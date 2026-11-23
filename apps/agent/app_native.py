"""Use native packages staged by the matching ANGI Buildroot image or administrator."""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import platform
import re
import shlex

import app_fetch
from app_store import AppError

PACKAGES = Path('/usr/share/agx/native-apps')
MICROSOFT_HOSTS = frozenset({'vscode.download.prss.microsoft.com', 'update.code.visualstudio.com'})


@dataclass(frozen=True)
class Package:
    release: app_fetch.Release
    archive: Path
    upstream: app_fetch.Release | None = None


class MissingPackage(AppError):
    """The current target has no staged native release for this app."""


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
        upstream = None
        if app_id == 'vscode':
            source = data['microsoft']
            source_size, source_digest = source['size'], source['sha256']
            if type(source_size) is not int or not 0 < source_size <= app_fetch.MAX_DOWNLOAD or not re.fullmatch(r'[0-9a-f]{64}', source_digest):
                raise ValueError()
            url = app_fetch.check_url(source['url'], MICROSOFT_HOSTS)
            upstream = app_fetch.Release(tag, 'code-linux-arm64.tar.gz', source_size, source_digest, url)
    except FileNotFoundError as error:
        raise MissingPackage('No native Buildroot package is available for this device.') from error
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise AppError('No native Buildroot package is available for this device.') from error
    return Package(app_fetch.Release(tag, archive.name, size, digest, ''), archive, upstream)


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


RUNTIME = {
    'blender': '''export VK_DRIVER_FILES="$root/lib/broadcom_icd.json"
set -- --gpu-backend vulkan "$@"
''',
    'chromium': '''export GDK_BACKEND=wayland
export GSETTINGS_SCHEMA_DIR="$root/share/glib-2.0/schemas"
set -- --ozone-platform=wayland --disable-setuid-sandbox "$@"
''',
    'vscode': '''export GDK_BACKEND=wayland
export LD_LIBRARY_PATH="$root/runtime/lib"
export XDG_DATA_DIRS="$root/runtime/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
set -- --ozone-platform=wayland --disable-setuid-sandbox "$@"
''',
    'gstreamer': (
        'export GST_PLUGIN_SYSTEM_PATH_1_0="$root/lib/gstreamer-1.0"\n'
        'export GST_PLUGIN_SCANNER="$root/libexec/gstreamer-1.0/gst-plugin-scanner"\n'
    ),
    'vlc': '''display=${WAYLAND_DISPLAY:-wayland-0}
case "$display" in
    /*) ;;
    *) display=${XDG_RUNTIME_DIR:-}/$display ;;
esac
case "${1:-}" in
    --version|--help|-h|-H|--longhelp) ;;
    *) if ! [ -S "$display" ] && ! [ -S "/proc/self/fd/${WAYLAND_SOCKET:-}" ]; then
           printf '%s\\n' 'VLC needs an ANGI Wayland session. On HDMI, run angi-wayland --next-login, then sign out and back in.' >&2
           exit 1
       fi ;;
esac
export QT_QPA_PLATFORM=wayland
export QT_PLUGIN_PATH="$root/lib/qt/plugins"
export VLC_PLUGIN_PATH="$root/lib/vlc/plugins"
export VLC_DATA_PATH="$root/share/vlc"
set -- --no-embedded-video --vout=wl_shm "$@"
''',
}


def command_wrapper(app_id, path):
    launcher = ''
    if app_id == 'vscode':
        # The vendor script's env interpreter is a setuid BusyBox applet on ANGI.
        # Run the script through Bash so the dynamic loader keeps the private runtime.
        launcher = '/bin/bash '
        path = str(Path(path).parent / 'bin/code')
    return (
        '#!/bin/sh\nset -eu\n'
        'root=$(dirname "$(dirname "$(readlink -f "$0")")")\n'
        'export LD_LIBRARY_PATH="$root/lib"\n'
        + RUNTIME.get(app_id, '') +
        f'exec {launcher}"$root"/{shlex.quote(path)} "$@"\n'
    )


def wrapper_is_current(app_id, tree, program):
    try:
        path = app_fetch.find_program(tree, program)
        return (tree / 'commands' / program.name).read_text() == command_wrapper(app_id, path)
    except (AppError, OSError, UnicodeError):
        return False


def command_wrappers(app_id, tree, programs):
    commands = tree / 'commands'
    commands.mkdir()
    for name, path in programs.items():
        facade = commands / name
        facade.write_text(command_wrapper(app_id, path))
        facade.chmod(0o755)
    return {name: f'commands/{name}' for name in programs}
