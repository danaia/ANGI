"""Qualify catalogue apps against the running ANGI device."""
import platform

import app_native
from app_store import AppError

ARCHITECTURES = {'aarch64': 'arm64', 'x86_64': 'amd64'}


def architecture():
    return ARCHITECTURES.get(platform.machine())


def unavailable(app):
    if not app.native:
        return '' if architecture() == 'arm64' else 'This release is built for 64-bit ARM Linux.'
    try:
        app_native.resolve(app.id)
    except app_native.MissingPackage as error:
        return {
            'blender': 'No Blender Wayland package for this ANGI build.',
            'chromium': 'No Chromium Wayland package for this ANGI build.',
            'vscode': 'No ANGI Buildroot package for the VS Code desktop runtime.',
            'vlc': 'No VLC Wayland package for this ANGI build.',
        }.get(app.id, str(error))
    except AppError as error:
        return str(error)
    return ''
