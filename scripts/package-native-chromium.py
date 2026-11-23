#!/usr/bin/env python3
"""Stage Raspberry Pi OS Chromium with a private runtime for ANGI."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile


# Keep the active graphics stack and the C/C++ runtime from the target image.
SYSTEM_LIBRARIES = {
    'ld-linux-aarch64.so.1', 'libc.so.6', 'libdl.so.2', 'libm.so.6',
    'libpthread.so.0', 'librt.so.1', 'libresolv.so.2', 'libgcc_s.so.1',
    'libatomic.so.1', 'libstdc++.so.6', 'libEGL.so.1', 'libGLESv2.so.2',
    'libGL.so.1', 'libdrm.so.2', 'libgbm.so.1', 'libudev.so.1',
    'libwayland-client.so.0', 'libwayland-cursor.so.0', 'libwayland-egl.so.1',
}
LOADED_LIBRARIES = {
    'libgtk-3.so.0', 'libnssckbi.so', 'libnssdbm3.so', 'libsoftokn3.so',
    'libfreebl3.so', 'libfreeblpriv3.so',
}
RUNTIME_FILES = {
    'chromium', 'chrome_crashpad_handler', 'chrome_100_percent.pak',
    'chrome_200_percent.pak', 'resources.pak', 'icudtl.dat',
    'snapshot_blob.bin', 'v8_context_snapshot.bin', 'locales',
    'libEGL.so', 'libGLESv2.so',
}


def dependencies(readelf, path):
    dynamic = subprocess.check_output([readelf, '-d', path], text=True)
    return set(re.findall(r'Shared library: \[([^\[\]]+)\]', dynamic))


def runtime_libraries(readelf, source, target, supplemental):
    pending = {name for name in LOADED_LIBRARIES if (target / name).is_file()}
    pending.add('libgtk-3.so.0')
    for name in ('chromium', 'chrome_crashpad_handler', 'libEGL.so', 'libGLESv2.so'):
        pending.update(dependencies(readelf, source / name))
    locations = (target, target / 'pulseaudio', supplemental)
    libraries = {}
    while pending:
        name = pending.pop()
        if name in SYSTEM_LIBRARIES or name in libraries:
            continue
        library = next((directory / name for directory in locations
                        if (directory / name).is_file()), None)
        if library is None:
            raise SystemExit(f'The Chromium runtime is missing {name}.')
        libraries[name] = library.resolve(strict=True)
        pending.update(dependencies(readelf, library))
    return libraries


def package(build, chromium, supplemental, version, destination):
    if (build / 'images/board.txt').read_text().strip() != 'rpi5':
        raise SystemExit('The Buildroot output does not target Raspberry Pi 5.')
    if not re.fullmatch(r'[0-9]+(?:\.[0-9]+){3}', version):
        raise SystemExit('Expected a four-part Chromium version.')
    readelf = build / 'host/bin/aarch64-linux-readelf'
    source = chromium / 'usr/lib/chromium'
    if not all((source / name).exists() for name in RUNTIME_FILES):
        raise SystemExit('Extract the matching Chromium and Chromium Common ARM64 packages first.')
    header = subprocess.check_output([readelf, '-h', source / 'chromium'], text=True)
    if not re.search(r'Machine:\s+AArch64\b', header):
        raise SystemExit('The Chromium executable does not target ARM64.')
    libraries = runtime_libraries(readelf, source, build / 'target/usr/lib',
                                  supplemental / 'usr/lib/aarch64-linux-gnu')
    identity = dict(line.split('=', 1) for line in
                    (build / 'target/etc/os-release').read_text().splitlines() if '=' in line)
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination) as directory:
        stage = Path(directory)
        schemas = build / 'target/usr/share/glib-2.0/schemas'
        subprocess.run([build / 'host/bin/glib-compile-schemas', '--strict',
                        '--targetdir', stage, schemas], check=True)
        archive = stage / 'chromium.tar.gz'
        with tarfile.open(archive, 'w:gz', compresslevel=4) as bundle:
            for name in sorted(RUNTIME_FILES):
                bundle.add(source / name, arcname=name)
            for name, library in sorted(libraries.items()):
                bundle.add(library, arcname=f'lib/{name}')
            bundle.add(stage / 'gschemas.compiled', arcname='share/glib-2.0/schemas/gschemas.compiled')
            for tree in (chromium, supplemental):
                for license_file in sorted((tree / 'usr/share/doc').glob('*/copyright')):
                    bundle.add(license_file.resolve(), arcname=f'licenses/{license_file.parent.name}/copyright')
        with archive.open('rb') as stream:
            checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
        manifest = stage / 'chromium.json'
        manifest.write_text(json.dumps(dict(board='rpi5', arch='aarch64',
                                            buildroot=identity['VERSION_ID'].strip('"'),
                                            version=version, sha256=checksum), indent=2) + '\n')
        archive.replace(destination / archive.name)
        manifest.replace(destination / manifest.name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path)
    parser.add_argument('chromium', type=Path, help='verified Raspberry Pi OS packages extracted together')
    parser.add_argument('supplemental', type=Path, help='verified Debian ARM64 runtime packages extracted together')
    parser.add_argument('version')
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    package(args.build, args.chromium, args.supplemental, args.version, args.destination)
