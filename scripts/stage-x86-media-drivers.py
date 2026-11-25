#!/usr/bin/env python3
"""Stage common x86 audio and USB Bluetooth bundles from build outputs."""

import argparse
import json
from pathlib import Path
import shutil
import tempfile

import x86_media_firmware


MODULES = (
    ("usb-audio", "sound/usb", "snd-usb-audio.ko"),
    ("hda-audio", "sound/pci/hda", "snd-hda-intel.ko"),
    ("usb-bluetooth", "drivers/bluetooth", "btusb.ko"),
)


def stage(args):
    output = args.output.resolve()
    if output.exists():
        raise ValueError(f"Bundle output already exists: {output}")
    firmware = x86_media_firmware.matching(args.firmware)
    manifest = []
    with tempfile.TemporaryDirectory(prefix=".media-drivers-", dir=output.parent) as work:
        build = Path(work) / "bundles"
        for name, directory, primary in MODULES:
            source = args.kernel_build / directory
            if not (source / primary).is_file():
                raise ValueError(f"Missing compiled driver: {source / primary}")
            modules = build / name / "modules"
            modules.mkdir(parents=True)
            for module in sorted(source.glob("*.ko")):
                shutil.copyfile(module, modules / module.name)
            manifest.append({"payload": name, "alias_modules": [primary]})
        firmware_target = build / "usb-bluetooth" / "firmware"
        for relative in firmware:
            destination = firmware_target / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(args.firmware / relative, destination)
        (build / "manifest.json").write_text(json.dumps({"bundles": manifest}, indent=2) + "\n")
        build.rename(output)
    print(output / "manifest.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel-build", type=Path, required=True)
    parser.add_argument("--firmware", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    try:
        stage(parser.parse_args())
    except (OSError, ValueError) as error:
        parser.exit(1, f"Media bundle staging failed: {error}\n")
