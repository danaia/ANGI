#!/usr/bin/env python3
"""Package the Pi 5 Buildroot VLC player with its private Wayland runtime."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import tarfile
import tempfile


def built_package(build, name):
    directories = [path.parent for path in (build / 'build').glob(f'{name}-*/.stamp_target_installed')]
    if len(directories) != 1:
        raise SystemExit(f'Expected one installed Buildroot build of {name}.')
    return directories[0]


def runtime_files(usr):
    if (usr / 'bin/vlc').is_file():
        yield 'vlc', usr / 'bin/vlc'
    for prefix in ('libvlc', 'libav', 'libsw', 'libQt5', 'libpcre2-16'):
        for path in (usr / 'lib').glob(f'{prefix}*.so.*'):
            if path.name.partition('.so.')[2].isdigit():
                yield path.relative_to(usr).as_posix(), path
    for directory in ('lib/vlc/plugins', 'lib/qt/plugins'):
        for path in (usr / directory).rglob('*.so'):
            if path.parent.name != 'platforms' or path.name.startswith('libqwayland'):
                yield path.relative_to(usr).as_posix(), path
    for path in (usr / 'share/vlc').rglob('*'):
        if path.is_file():
            yield path.relative_to(usr).as_posix(), path


def pack(build, destination):
    config = set((build / '.config').read_text().splitlines())
    required = ('BR2_aarch64=y', 'BR2_cortex_a76=y', 'BR2_PACKAGE_VLC=y',
                'BR2_PACKAGE_FFMPEG=y', 'BR2_PACKAGE_QT5BASE_WIDGETS=y',
                'BR2_PACKAGE_QT5WAYLAND=y', 'BR2_PACKAGE_WAYLAND_PROTOCOLS=y')
    if not set(required) <= config or (build / 'images/board.txt').read_text().strip() != 'rpi5':
        raise SystemExit('Build the native VLC targets for Raspberry Pi 5 first.')
    names = ('vlc', 'ffmpeg', 'qt5base', 'qt5wayland', 'qt5svg', 'pcre2')
    components = {name: built_package(build, name) for name in names}
    files = dict(runtime_files(build / 'target/usr'))
    needed = {'vlc', 'lib/qt/plugins/platforms/libqwayland-generic.so'}
    if not needed <= files.keys() or not any(path.endswith('/libqt_plugin.so') for path in files):
        raise SystemExit('The Buildroot output is missing VLC’s graphical interface or Qt Wayland plugin.')
    video_plugins = {'libwl_shm_plugin.so', 'libxdg_shell_plugin.so'}
    if not video_plugins <= {Path(path).name for path in files}:
        raise SystemExit('The Buildroot output is missing the VLC Wayland video output plugin.')

    identity = dict(line.split('=', 1) for line in
                    (build / 'target/etc/os-release').read_text().splitlines() if '=' in line)
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination) as directory:
        stage = Path(directory)
        archive = stage / 'vlc.tar.gz'
        with tarfile.open(archive, 'w:gz') as bundle:
            for relative, source in sorted(files.items()):
                real = source.resolve(strict=True)
                if relative == 'vlc' or relative.startswith('lib/'):
                    binary = stage / 'stripped'
                    subprocess.run([build / 'host/bin/aarch64-linux-strip', '--strip-unneeded',
                                    '-o', binary, real], check=True)
                    bundle.add(binary, arcname=relative)
                else:
                    bundle.add(real, arcname=relative)
            for name, source in components.items():
                for pattern in ('COPYING*', 'LICENSE*', 'LICENCE*'):
                    for license_file in sorted(source.glob(pattern)):
                        if license_file.is_file():
                            bundle.add(license_file, arcname=f'licenses/{name}/{license_file.name}')
        with archive.open('rb') as stream:
            checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
        manifest = stage / 'vlc.json'
        manifest.write_text(json.dumps(dict(board='rpi5', arch='aarch64',
                                            buildroot=shlex.split(identity['VERSION_ID'])[0],
                                            version=components['vlc'].name.removeprefix('vlc-'),
                                            sha256=checksum), indent=2) + '\n')
        archive.replace(destination / archive.name)
        manifest.replace(destination / manifest.name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    pack(args.build, args.destination)
