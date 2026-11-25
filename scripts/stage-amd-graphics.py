#!/usr/bin/env python3
"""Add a Radeon Renoir/Lucienne graphics bundle to an x86 driver manifest."""

import argparse
import json
from pathlib import Path
import shutil
import tempfile


def stage(kernel, firmware, graphics, manifest):
    output = manifest.parent / "amd-renoir"
    if output.exists():
        raise ValueError(f"Bundle output already exists: {output}")
    data = json.loads(manifest.read_text())
    with tempfile.TemporaryDirectory(prefix=".amd-drivers-", dir=manifest.parent) as temp:
        bundle = Path(temp) / output.name
        modules = bundle / "modules"
        modules.mkdir(parents=True)
        module_root = kernel / "drivers/gpu/drm/amd"
        for module in ("amdgpu", "amdxcp"):
            shutil.copyfile(module_root / module / f"{module}.ko", modules / f"{module}.ko")
        blobs = bundle / "firmware/amdgpu"
        blobs.mkdir(parents=True)
        for family in ("renoir", "green_sardine"):
            files = sorted((firmware / "amdgpu").glob(f"{family}_*.bin"))
            if not files:
                raise ValueError(f"Missing {family} firmware in {firmware}")
            for file in files:
                shutil.copyfile(file, blobs / file.name)
        shutil.copytree(graphics, bundle / "graphics")
        data["bundles"].append({
            "payload": output.name,
            "devices": [
                {"bus": "pci", "vendor": 0x1002, "product": device}
                for device in (0x1636, 0x164C)
            ],
        })
        updated = Path(temp) / manifest.name
        updated.write_text(json.dumps(data, indent=2) + "\n")
        bundle.rename(output)
        updated.replace(manifest)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel-build", type=Path, required=True)
    parser.add_argument("--firmware", type=Path, required=True)
    parser.add_argument("--graphics", type=Path, required=True,
                        help="Mesa graphics payload built for ANGI with RadeonSI enabled")
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    try:
        stage(args.kernel_build, args.firmware, args.graphics, args.manifest)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"AMD graphics staging failed: {error}\n")
