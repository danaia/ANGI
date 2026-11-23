#!/usr/bin/env python3
"""Package a portable Linux ARM64 Blender build and the matching Pi 5 Vulkan driver."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile


# The upstream USD library links these client libraries. Blender's
# Wayland viewport uses Vulkan through the bundled Broadcom driver.
USD_CLIENT_LIBRARIES = (
    'libGL.so.1', 'libGLX.so.0', 'libGLdispatch.so.0',
    'libSM.so.6', 'libICE.so.6', 'libX11.so.6', 'libXext.so.6',
    'libxcb.so.1', 'libXau.so.6', 'libXdmcp.so.6',
)
USD_CLIENT_PACKAGES = (
    'libxcb', 'xlib_libICE', 'xlib_libSM', 'xlib_libX11',
    'xlib_libXext', 'xlib_libXau', 'xlib_libXdmcp',
)


def package(build, blender, driver, loader, version, destination):
    config = set((build / '.config').read_text().splitlines())
    required = {'BR2_aarch64=y', 'BR2_cortex_a76=y', 'BR2_PACKAGE_AGX_WAYLAND=y'}
    if not required <= config or (build / 'images/board.txt').read_text().strip() != 'rpi5':
        raise SystemExit('Use a Pi 5 Buildroot output with the native Wayland session.')
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version):
        raise SystemExit('Use the matching Blender release version.')
    release_data = blender / '.'.join(version.split('.')[:2])
    if not (release_data / 'scripts').is_dir() or not (blender / 'license').is_dir():
        raise SystemExit('Use the complete portable Blender install tree.')
    programs = {'blender': blender / 'blender', 'lib/libvulkan.so.1': loader,
                'lib/libvulkan_broadcom.so': driver}
    uses_usd = (blender / 'lib/libusd_ms.so').is_file()
    if uses_usd:
        for name in USD_CLIENT_LIBRARIES:
            library = build / 'target/usr/lib' / name
            if not library.is_file():
                raise SystemExit(f'Build the Pi 5 Blender client runtime to provide {name}.')
            programs[f'lib/{name}'] = library
    readelf = build / 'host/bin/aarch64-linux-readelf'
    for name, binary in programs.items():
        header = subprocess.check_output([readelf, '-h', binary], text=True)
        if not re.search(r'Machine:\s+AArch64\b', header):
            raise SystemExit(f'{name} must be a Linux ARM64 binary.')
    identity = dict(line.split('=', 1) for line in
                    (build / 'target/etc/os-release').read_text().splitlines() if '=' in line)
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination) as temporary:
        stage = Path(temporary)
        icd = stage / 'broadcom_icd.json'
        icd.write_text(json.dumps({
            'file_format_version': '1.0.0',
            'ICD': {'library_path': './libvulkan_broadcom.so', 'api_version': '1.3.0'},
        }) + '\n')
        with tarfile.open(stage / 'blender.tar.gz', 'w:gz', compresslevel=4) as archive:
            for name in (release_data.name, 'license', 'lib'):
                tree = blender / name
                if tree.is_dir():
                    archive.add(tree, arcname=name, filter=lambda item: item if item.name not in programs else None)
            for name, binary in programs.items():
                stripped = stage / 'stripped'
                subprocess.run([build / 'host/bin/aarch64-linux-strip', '--strip-unneeded',
                                '-o', stripped, binary.resolve(strict=True)], check=True)
                archive.add(stripped, arcname=name)
            archive.add(icd, arcname='lib/broadcom_icd.json')
            licenses = {'mesa-license.rst': 'mesa3d-*/docs/license.rst'}
            if uses_usd:
                licenses['libglvnd-README.md'] = 'libglvnd-*/README.md'
                licenses.update({f'{package}-COPYING': f'{package}-*/COPYING'
                                 for package in USD_CLIENT_PACKAGES})
            for name, pattern in licenses.items():
                path = next((build / 'build').glob(pattern), None)
                if path is not None:
                    archive.add(path, arcname=f'license/{name}')
        with (stage / 'blender.tar.gz').open('rb') as stream:
            checksum = hashlib.file_digest(stream, 'sha256').hexdigest()
        (stage / 'blender.json').write_text(json.dumps({
            'board': 'rpi5', 'arch': 'aarch64', 'buildroot': identity['VERSION_ID'].strip('"'),
            'version': version, 'sha256': checksum,
        }, indent=2) + '\n')
        for name in ('blender.tar.gz', 'blender.json'):
            (stage / name).replace(destination / name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path)
    parser.add_argument('blender', type=Path, help='portable CMake install prefix')
    parser.add_argument('driver', type=Path, help='matching Mesa libvulkan_broadcom.so')
    parser.add_argument('loader', type=Path, help='native libvulkan.so.1')
    parser.add_argument('version')
    parser.add_argument('destination', type=Path)
    package(**vars(parser.parse_args()))
