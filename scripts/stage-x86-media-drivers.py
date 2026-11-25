#!/usr/bin/env python3
"""Stage common x86 audio and USB Bluetooth bundles from build outputs."""

import argparse
import json
from pathlib import Path
import shutil
import tempfile


MODULES = (
    ("usb-audio", "sound/usb", "snd-usb-audio.ko"),
    ("hda-audio", "sound/pci/hda", "snd-hda-intel.ko"),
    ("usb-bluetooth", "drivers/bluetooth", "btusb.ko"),
)
FIRMWARE = (
    "rtl_bt/*", "intel/ibt-*", "mediatek/**/BT_RAM_CODE*.bin",
    "mediatek/mt76*pr*h.bin", "qca/nvm_usb_*", "qca/rampatch_usb_*",
    "brcm/BCM*.hcd", "ar3k/*",
)


def stage(args):
    output = args.output.resolve()
    if output.exists():
        raise ValueError(f"Bundle output already exists: {output}")
    if not (args.firmware / "WHENCE").is_file():
        raise ValueError("Use the unpacked linux-firmware source tree")
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
        for pattern in FIRMWARE:
            for source in sorted(args.firmware.glob(pattern)):
                if source.is_file():
                    destination = firmware_target / source.relative_to(args.firmware)
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, destination)
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
