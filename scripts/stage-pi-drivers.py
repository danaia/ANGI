#!/usr/bin/env python3
"""Stage optional USB Bluetooth firmware for the Raspberry Pi driver feed."""

import argparse
import json
from pathlib import Path
import shutil
import tempfile


def stage(firmware, output):
    if output.exists():
        raise ValueError(f"Bundle output already exists: {output}")
    if not (firmware / "WHENCE").is_file():
        raise ValueError("Use the unpacked linux-firmware source tree")

    # These USB IDs use RTL8761BU firmware in Linux's btrtl driver.
    bundle = {
        "payload": "rtl8761bu",
        "devices": [
            {"bus": "usb", "vendor": vendor, "product": 0x8771}
            for vendor in (0x0BDA, 0x6655)
        ],
    }
    with tempfile.TemporaryDirectory(prefix=".pi-drivers-", dir=output.parent) as temp:
        build = Path(temp) / "bundles"
        target = build / bundle["payload"] / "firmware/rtl_bt"
        target.mkdir(parents=True)
        for filename in ("rtl8761bu_fw.bin", "rtl8761bu_config.bin"):
            shutil.copyfile(firmware / "rtl_bt" / filename, target / filename)
        (build / "manifest.json").write_text(json.dumps({"bundles": [bundle]}, indent=2) + "\n")
        build.rename(output)
    print(output / "manifest.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--firmware", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        stage(args.firmware, args.output.resolve())
    except (OSError, ValueError) as error:
        parser.exit(1, f"Pi driver bundle staging failed: {error}\n")
