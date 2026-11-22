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
    if app.kind == 'Desktop':
        return 'Awaiting native ANGI window support.'
    try:
        app_native.resolve(app.id)
    except AppError as error:
        return str(error)
    return ''
