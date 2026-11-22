#!/usr/bin/env python3
"""Package the Pi 5 Buildroot GStreamer targets for the ANGI app installer."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import tarfile
import tempfile


def pack(build, destination):
    config = (build / '.config').read_text().splitlines()
    required = ('BR2_aarch64=y', 'BR2_cortex_a76=y', 'BR2_PACKAGE_GSTREAMER1=y',
                'BR2_PACKAGE_GST1_PLUGINS_BASE=y', 'BR2_PACKAGE_GST1_PLUGINS_GOOD_PLUGIN_PULSE=y')
    if not set(required).issubset(config) or (build / 'images/board.txt').read_text().strip() != 'rpi5':
        raise SystemExit('Build the Pi 5 GStreamer, base plugins and PulseAudio plugin targets first.')

    usr = build / 'target/usr'
    identity = dict(line.split('=', 1) for line in (build / 'target/etc/os-release').read_text().splitlines() if '=' in line)
    strip = build / 'host/bin/aarch64-linux-strip'
    license_file, = (build / 'build').glob('gstreamer1-*/COPYING')
    version = license_file.parent.name.removeprefix('gstreamer1-')
    destination.mkdir(parents=True, exist_ok=True)
    programs = (
        (usr / 'bin/gst-inspect-1.0', 'libexec/gst-inspect-1.0'),
        (usr / 'bin/gst-launch-1.0', 'libexec/gst-launch-1.0'),
        (usr / 'libexec/gstreamer-1.0/gst-plugin-scanner', 'libexec/gstreamer-1.0/gst-plugin-scanner'),
    )
    libraries = sorted((usr / 'lib').glob('libgst*.so.0'))
    plugins = sorted((usr / 'lib/gstreamer-1.0').glob('*.so'))
    if not libraries or not {'libgstcoreelements.so', 'libgstaudiotestsrc.so', 'libgstpulseaudio.so'} <= {file.name for file in plugins}:
        raise SystemExit('The Buildroot output is missing required GStreamer libraries or plugins.')
    packaged = [
        *programs,
        *((file, 'lib/' + file.name) for file in libraries),
        *((file, 'lib/gstreamer-1.0/' + file.name) for file in plugins),
    ]
    with tempfile.TemporaryDirectory(dir=destination) as directory:
        stage = Path(directory)
        archive = stage / 'gstreamer.tar.gz'
        with tarfile.open(archive, 'w:gz') as package:
            for original, relative in packaged:
                binary = stage / 'stripped'
                subprocess.run([strip, '--strip-unneeded', '-o', binary, original.resolve(strict=True)], check=True)
                package.add(binary, arcname=relative)
            for name in ('gstreamer1', 'gst1-plugins-base', 'gst1-plugins-good'):
                package.add(build / f'build/{name}-{version}/COPYING', arcname=f'licenses/{name}.txt')
        with archive.open('rb') as stream:
            checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
        manifest = stage / 'gstreamer.json'
        manifest.write_text(json.dumps(dict(board='rpi5', arch='aarch64', buildroot=shlex.split(identity['VERSION_ID'])[0],
                                            version=version, sha256=checksum), indent=2) + '\n')
        archive.replace(destination / archive.name)
        manifest.replace(destination / manifest.name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    pack(args.build, args.destination)
