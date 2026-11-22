"""Use native packages staged by the matching ANGI Buildroot image or administrator."""
import hashlib
import json
from pathlib import Path
import platform
import re
import shlex

import app_fetch
from app_store import AppError

PACKAGES = Path('/usr/share/agx/native-apps')


def target():
    try:
        model = Path('/proc/device-tree/model').read_text().rstrip('\0')
        fields = dict(line.split('=', 1) for line in Path('/etc/os-release').read_text().splitlines() if '=' in line)
        if model.startswith('Raspberry Pi 5 Model') and platform.machine() == 'aarch64' and fields.get('ID') == 'buildroot':
            return 'rpi5', 'aarch64', fields['VERSION_ID'].strip('"')
    except (OSError, KeyError):
        pass
    return None


def resolve(app_id):
    manifest = PACKAGES / f'{app_id}.json'
    archive = PACKAGES / f'{app_id}.tar.gz'
    try:
        installed_target = target()
        if not installed_target:
            raise AppError('No native Buildroot package is available for this device.')
        data = json.loads(manifest.read_text())
        if (data['board'], data['arch'], data['buildroot']) != installed_target:
            raise AppError('The native package targets a different ANGI build.')
        tag, digest, size = data['version'], data['sha256'], archive.stat().st_size
        if not app_fetch.TAG.fullmatch(tag) or not re.fullmatch(r'[0-9a-f]{64}', digest) or not 0 < size <= app_fetch.MAX_DOWNLOAD:
            raise ValueError()
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise AppError('No native Buildroot package is available for this device.') from error
    return app_fetch.Release(tag, archive.name, size, digest, ''), archive


def copy_package(archive, asset, size):
    digest = hashlib.sha256()
    remaining = size
    with archive.open('rb') as source, asset.open('wb') as output:
        while remaining:
            block = source.read(min(remaining, app_fetch.CHUNK))
            if not block:
                raise AppError('The native package changed while it was being copied.')
            digest.update(block)
            output.write(block)
            remaining -= len(block)
        if source.read(1):
            raise AppError('The native package changed while it was being copied.')
    return digest.hexdigest()


def command_wrappers(tree, programs):
    commands = tree / 'commands'
    commands.mkdir()
    for name, path in programs.items():
        facade = commands / name
        facade.write_text(
            '#!/bin/sh\nset -eu\n'
            'root=$(dirname "$(dirname "$(readlink -f "$0")")")\n'
            'export LD_LIBRARY_PATH="$root/lib"\n'
            'export GST_PLUGIN_SYSTEM_PATH_1_0="$root/lib/gstreamer-1.0"\n'
            'export GST_PLUGIN_SCANNER="$root/libexec/gstreamer-1.0/gst-plugin-scanner"\n'
            f'exec "$root"/{shlex.quote(path)} "$@"\n'
        )
        facade.chmod(0o755)
    return {name: f'commands/{name}' for name in programs}
