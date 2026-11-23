#!/usr/bin/env python3
"""Package Buildroot libraries for Microsoft's separate Linux ARM64 download."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile


# These come from the target's C library and its active Mesa/Wayland driver.
SYSTEM_LIBRARIES = frozenset({
    'ld-linux-aarch64.so.1', 'libc.so.6', 'libdl.so.2', 'libm.so.6',
    'libpthread.so.0', 'librt.so.1', 'libresolv.so.2', 'libgcc_s.so.1', 'libatomic.so.1',
    'libstdc++.so.6', 'libEGL.so.1', 'libGLESv2.so.2', 'libGL.so.1',
    'libdrm.so.2', 'libgbm.so.1', 'libudev.so.1', 'libwayland-client.so.0',
    'libwayland-cursor.so.0', 'libwayland-egl.so.1', 'libwayland-server.so.0',
})
NSS_MODULES = ('libnssckbi.so', 'libnssdbm3.so', 'libsoftokn3.so', 'libfreebl3.so', 'libfreeblpriv3.so')


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def needed(readelf, path):
    dynamic = subprocess.check_output([readelf, '-d', path], text=True)
    return re.findall(r'Shared library: \[([^\[\]]+)\]', dynamic)


def runtime_files(build, binary, bundled):
    readelf = build / 'host/bin/aarch64-linux-readelf'
    library_dir = build / 'target/usr/lib'
    pending = needed(readelf, binary)
    pending.extend(name for name in NSS_MODULES if (library_dir / name).is_file())
    files = {}
    while pending:
        name = pending.pop()
        if name in SYSTEM_LIBRARIES or name in files or name in bundled:
            continue
        library = library_dir / name
        if not library.is_file():
            raise SystemExit(f'The Buildroot output is missing {name}.')
        files[name] = library.resolve(strict=True)
        pending.extend(needed(readelf, library))
    return files


def package(build, source, metadata_path, destination):
    config = (build / '.config').read_text().splitlines()
    required = ('BR2_aarch64=y', 'BR2_cortex_a76=y', 'BR2_PACKAGE_LIBGTK3_WAYLAND=y',
                'BR2_PACKAGE_AGX_WAYLAND=y')
    if any(option not in config for option in required):
        raise SystemExit('Build the Pi 5 native VS Code runtime first.')
    if (build / 'images/board.txt').read_text().strip() != 'rpi5':
        raise SystemExit('The Buildroot output does not target Raspberry Pi 5.')

    metadata = json.loads(metadata_path.read_text())
    if sha256(source) != metadata['sha256hash']:
        raise SystemExit('The VS Code download differs from Microsoft’s checksum.')
    identity = dict(line.split('=', 1) for line in
                    (build / 'target/etc/os-release').read_text().splitlines() if '=' in line)
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination) as temporary:
        stage = Path(temporary)
        with tarfile.open(source, 'r:gz') as upstream:
            cli = upstream.getmember('VSCode-linux-arm64/bin/code')
            if not cli.isfile() or not cli.mode & 0o100:
                raise SystemExit('The official archive is missing the VS Code command-line launcher.')
            binary_data = upstream.extractfile('VSCode-linux-arm64/code')
            binary = stage / 'code'
            binary.write_bytes(binary_data.read())
            bundled = {Path(name).name for name in upstream.getnames()
                       if Path(name).parent.as_posix() == 'VSCode-linux-arm64'}
        header = subprocess.check_output([build / 'host/bin/aarch64-linux-readelf', '-h', binary], text=True)
        if not re.search(r'Machine:\s+AArch64\b', header):
            raise SystemExit('The official archive does not contain an ARM64 executable.')

        archive = stage / 'vscode.tar.gz'
        files = runtime_files(build, binary, bundled)
        with tarfile.open(archive, 'w:gz') as target:
            for name, library in sorted(files.items()):
                stripped = stage / 'stripped'
                subprocess.run([build / 'host/bin/aarch64-linux-strip', '--strip-unneeded',
                                '-o', stripped, library], check=True)
                target.add(stripped, arcname=f'lib/{name}')
            schemas = build / 'target/usr/share/glib-2.0/schemas'
            if schemas.is_dir() and any(schemas.glob('*.gschema.xml')):
                local_schemas = stage / 'schemas'
                shutil.copytree(schemas, local_schemas)
                subprocess.run([build / 'host/bin/glib-compile-schemas', '--strict', local_schemas], check=True)
                target.add(local_schemas / 'gschemas.compiled',
                           arcname='share/glib-2.0/schemas/gschemas.compiled')
        manifest = stage / 'vscode.json'
        manifest.write_text(json.dumps({
            'board': 'rpi5', 'arch': 'aarch64', 'buildroot': identity['VERSION_ID'].strip('"'),
            'version': metadata['productVersion'], 'sha256': sha256(archive),
            'microsoft': {'url': metadata['url'], 'sha256': metadata['sha256hash'],
                          'size': source.stat().st_size},
        }, indent=2) + '\n')
        archive.replace(destination / archive.name)
        manifest.replace(destination / manifest.name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path)
    parser.add_argument('source', type=Path, help='Microsoft’s original Linux ARM64 tarball')
    parser.add_argument('metadata', type=Path, help='Microsoft’s update metadata for this tarball')
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    package(args.build, args.source, args.metadata, args.destination)
