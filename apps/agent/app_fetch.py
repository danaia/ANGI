"""Find a release asset on GitHub, download it against its published checksum, and unpack it safely.

Nothing downloaded is ever executed here. Archives are untrusted: member paths, links and sizes are
checked before anything is written, and each program is confirmed to be a 64-bit ARM Linux ELF file.
"""
import bz2
import gzip
import hashlib
import lzma
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import socket
import ssl
import tarfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

import update_sources as upstream
from app_store import AppError

DOWNLOAD_HOSTS = {'github.com', 'objects.githubusercontent.com',
                  'release-assets.githubusercontent.com', 'github-releases.githubusercontent.com'}
MAX_DOWNLOAD = 512 * 1024 * 1024
MAX_UNPACKED = 2 * 1024 ** 3
MAX_ENTRIES = 20000
CHUNK = 256 * 1024
ATTEMPTS = 3
TAG = re.compile(r'[A-Za-z0-9][A-Za-z0-9._+-]{0,63}')
EM_AARCH64 = 183


class Release:
    def __init__(self, tag, asset, size, sha256, url):
        self.tag, self.asset, self.size, self.sha256, self.url = tag, asset, size, sha256, url


def friendly(error):
    """A message for the person, not a traceback."""
    kind = getattr(error, 'retry_kind', '')
    if kind == 'network':
        return 'Cannot reach GitHub. Check the network connection and try again.'
    if kind == 'clock':
        return 'The system clock has not synchronized yet. Try again in a minute.'
    if kind == 'rate-limit':
        return 'GitHub is limiting requests right now. Try again in a little while.'
    return str(error)


def check_url(url):
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != 'https' or parsed.hostname not in DOWNLOAD_HOSTS:
        raise AppError('The download was redirected somewhere unexpected, so it was stopped.')
    return url


def resolve(app, get=upstream.fetch):
    """The newest stable release of an app and the one asset that is its aarch64 Linux build."""
    try:
        data = get(upstream.GITHUB + app.repo + '/releases/latest')
    except upstream.TemporarilyUnavailable as error:
        raise AppError(friendly(error)) from error
    except (RuntimeError, ValueError) as error:
        raise AppError(f'Could not read the latest {app.name} release: {error}') from error
    tag = data.get('tag_name')
    if data.get('draft') or data.get('prerelease') or not isinstance(tag, str) or not TAG.fullmatch(tag):
        raise AppError(f'{app.name} has no stable release to install right now.')
    matches = [asset for asset in data.get('assets', []) if app.asset.search(asset.get('name', ''))]
    if not matches:
        raise AppError(f'{app.name} {tag} has no download for this computer (64-bit ARM Linux).')
    if len(matches) > 1:
        raise AppError(f'{app.name} {tag} offers several matching downloads; it cannot choose safely.')
    asset = matches[0]
    digest = re.fullmatch(r'sha256:([0-9a-f]{64})', asset.get('digest') or '')
    size = asset.get('size')
    if not digest:
        raise AppError(f'GitHub published no checksum for {app.name} {tag}, so it will not be installed.')
    if not isinstance(size, int) or not 0 < size <= MAX_DOWNLOAD:
        raise AppError(f'{app.name} {tag} has an unexpected download size.')
    return Release(tag, asset['name'], size, digest[1], check_url(asset.get('browser_download_url', '')))


class Redirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, new_url):
        check_url(new_url)
        return super().redirect_request(request, fp, code, msg, headers, new_url)


def download(release, destination, progress, opener=None, pause=time.sleep):
    """Stream the asset to `destination`; returns its SHA-256. Retries transient network failures."""
    opener = opener or urllib.request.build_opener(Redirects)
    request = urllib.request.Request(release.url, headers={'User-Agent': 'ANGI-apps/1', 'Accept': 'application/octet-stream'})
    last = None
    for attempt in range(ATTEMPTS):
        digest, received = hashlib.sha256(), 0
        try:
            with opener.open(request, timeout=30) as response, open(destination, 'wb') as out:
                os.chmod(destination, 0o600)
                check_url(response.url)
                while True:
                    block = response.read(CHUNK)
                    if not block:
                        break
                    received += len(block)
                    if received > release.size:
                        raise AppError('The download was larger than GitHub said it would be, so it was stopped.')
                    digest.update(block)
                    out.write(block)
                    progress(received, release.size)
            if received != release.size:
                raise OSError('The download ended early.')
            return digest.hexdigest()
        except AppError:
            raise
        except urllib.error.HTTPError as error:
            raise AppError(f'GitHub refused the download (HTTP {error.code}).') from error
        except (urllib.error.URLError, OSError, socket.timeout, TimeoutError) as error:
            reason = getattr(error, 'reason', error)
            if isinstance(reason, ssl.SSLCertVerificationError):
                raise AppError(friendly(upstream.TemporarilyUnavailable('', 'clock'))
                               if getattr(reason, 'verify_code', None) == 9
                               else 'The connection to GitHub could not be verified, so nothing was downloaded.') from error
            last = error
            if attempt + 1 < ATTEMPTS:
                pause(1 + 2 * attempt)
    raise AppError('Cannot reach GitHub. Check the network connection and try again.') from last


def verify(release, actual):
    if actual != release.sha256:
        raise AppError('The package did not match its published checksum, so it was discarded.')


def inside(root, path):
    return path == root or path.startswith(root + os.sep)


def sanitize_name(name):
    pure = PurePosixPath(name)
    if pure.is_absolute() or '..' in pure.parts or '\0' in name:
        raise AppError('The download contains an unsafe file path, so it was not unpacked.')
    return pure.parts


def unpack_tar(archive, root):
    root = os.path.realpath(root)
    with tarfile.open(archive, 'r:*') as tar:
        members = tar.getmembers()
        if len(members) > MAX_ENTRIES:
            raise AppError('The download contains too many files to unpack safely.')
        total = 0
        for member in members:
            parts = sanitize_name(member.name)
            if not parts:
                continue        # the archive's own "./" root entry
            destination = os.path.join(root, *parts)
            if member.isfile():
                total += member.size
                if total > MAX_UNPACKED:
                    raise AppError('The download unpacks to more than the safety limit.')
            elif member.issym():
                if os.path.isabs(member.linkname) or not inside(root, os.path.normpath(os.path.join(os.path.dirname(destination), member.linkname))):
                    raise AppError('The download contains a link that points outside itself, so it was not unpacked.')
            elif member.islnk():
                sanitize_name(member.linkname)
            elif not member.isdir():
                raise AppError('The download contains a special file, so it was not unpacked.')
            if not inside(root, os.path.realpath(os.path.dirname(destination))):
                raise AppError('The download tries to write outside its folder, so it was not unpacked.')
            # Owner-only write, no setuid/setgid/sticky; directories stay traversable.
            member.mode = (member.mode & 0o755) | (0o700 if member.isdir() else 0o600)
            member.uid, member.gid, member.uname, member.gname = os.getuid(), os.getgid(), '', ''
            tar.extract(member, root, **({'filter': 'tar'} if hasattr(tarfile, 'tar_filter') else {}))


def unpack_zip(archive, root):
    root = os.path.realpath(root)
    with zipfile.ZipFile(archive) as bundle:
        infos = bundle.infolist()
        if len(infos) > MAX_ENTRIES:
            raise AppError('The download contains too many files to unpack safely.')
        total = 0
        for info in infos:
            parts = sanitize_name(info.filename)
            if not parts:
                continue
            destination = os.path.join(root, *parts)
            if not inside(root, os.path.realpath(os.path.dirname(destination))):
                raise AppError('The download tries to write outside its folder, so it was not unpacked.')
            if info.is_dir():
                os.makedirs(destination, mode=0o755, exist_ok=True)
                continue
            if (info.external_attr >> 28) == 0xA:      # a symbolic link
                raise AppError('The download contains a link, so it was not unpacked.')
            os.makedirs(os.path.dirname(destination), mode=0o755, exist_ok=True)
            with bundle.open(info) as source, open(destination, 'wb') as out:
                for block in iter(lambda: source.read(CHUNK), b''):
                    total += len(block)
                    if total > MAX_UNPACKED:
                        raise AppError('The download unpacks to more than the safety limit.')
                    out.write(block)
            mode = (info.external_attr >> 16) & 0o755
            os.chmod(destination, (mode or 0o644) | 0o600)


def unpack_single(archive, name, root):
    opener = {'.gz': gzip.open, '.xz': lzma.open, '.bz2': bz2.open}[os.path.splitext(name)[1]]
    destination = Path(root) / os.path.splitext(name)[0]
    total = 0
    with opener(archive, 'rb') as source, open(destination, 'wb') as out:
        for block in iter(lambda: source.read(CHUNK), b''):
            total += len(block)
            if total > MAX_UNPACKED:
                raise AppError('The download unpacks to more than the safety limit.')
            out.write(block)


def unpack(archive, asset, root):
    root = Path(root)
    root.mkdir(mode=0o755, parents=True, exist_ok=True)
    lower = asset.lower()
    try:
        if lower.endswith(('.tar.gz', '.tgz', '.tar.xz', '.txz', '.tar.bz2', '.tbz')):
            unpack_tar(archive, root)
        elif lower.endswith('.zip'):
            unpack_zip(archive, root)
        elif lower.endswith(('.gz', '.xz', '.bz2')):
            unpack_single(archive, asset, root)
        else:
            shutil.copyfile(archive, root / asset)
    except (tarfile.TarError, zipfile.BadZipFile, EOFError, OSError, lzma.LZMAError) as error:
        raise AppError(f'The download could not be unpacked: {error}') from error


def find_program(root, program):
    """Path of a program inside the unpacked release, relative to it. Exactly one file must match."""
    root = Path(os.path.realpath(root))
    if program.path:
        found = [path for path in root.glob(program.path) if path.is_file()]
    else:
        found = [path for path in root.iterdir() if path.is_file()]
    found = [path for path in found if inside(str(root), os.path.realpath(path))]
    if len(found) != 1:
        raise AppError(f'The release does not contain {program.name} where expected.')
    return found[0].relative_to(root).as_posix()


def check_program(path, name):
    """A 64-bit little-endian ARM Linux ELF file; read, never run."""
    with open(path, 'rb') as stream:
        head = stream.read(20)
    if len(head) < 20 or head[:4] != b'\x7fELF' or head[4] != 2 or head[5] != 1 or int.from_bytes(head[18:20], 'little') != EM_AARCH64:
        raise AppError(f'{name} in this download is not a 64-bit ARM Linux program, so it was not installed.')
    os.chmod(path, 0o755)
