#!/usr/bin/env python3
"""USB Bluetooth firmware that x86 builds take from the linux-firmware source."""

import argparse
from pathlib import Path
import shutil


# Buildroot has no package option for many of these files.
PATTERNS = (
    "rtl_bt/*", "intel/ibt-*", "mediatek/**/BT_RAM_CODE*.bin",
    "mediatek/mt76*pr*h.bin", "qca/nvm_usb_*", "qca/rampatch_usb_*",
    "brcm/BCM*.hcd", "ar3k/*",
)


def matching(source):
    """Return the matching files, relative to the unpacked source tree."""
    if not (source / "WHENCE").is_file():
        raise ValueError("Use the unpacked linux-firmware source tree")
    return sorted({path.relative_to(source) for pattern in PATTERNS
                   for path in source.glob(pattern) if path.is_file()})


def _copy(source, destination):
    for parent in reversed(destination.parents):
        if not parent.exists():
            parent.mkdir()
            parent.chmod(0o755)
    shutil.copyfile(source, destination)
    destination.chmod(0o644)


def install(source, rootfs):
    """Add the files a root filesystem lacks, with their redistribution notices."""
    for name in matching(source):
        destination = rootfs / "lib/firmware" / name
        # Keep whatever Buildroot installed, including its firmware links.
        if not destination.exists() and not destination.is_symlink():
            _copy(source / name, destination)
    notices = [source / "WHENCE", *sorted(source.glob("LICEN[CS]E*")),
               *sorted((source / "LICENSES").rglob("*"))]
    for notice in notices:
        if notice.is_file():
            _copy(notice, rootfs / "usr/share/licenses/linux-firmware" / notice.relative_to(source))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("firmware", type=Path, help="unpacked linux-firmware source")
    parser.add_argument("rootfs", type=Path, help="target root filesystem")
    arguments = parser.parse_args()
    try:
        install(arguments.firmware, arguments.rootfs)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Bluetooth firmware installation failed: {error}\n")
