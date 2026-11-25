#!/usr/bin/env python3
"""Package driver files and sign the catalog for an exact ANGI kernel build."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.parse import urlsplit


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def kernel_identity(build, objcopy, work):
    release = (build / "include/config/kernel.release").read_text().strip()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.+-]*", release):
        raise ValueError("Invalid kernel release")
    notes = work / "kernel.notes"
    run(objcopy, "--dump-section", f".notes={notes}", build / "vmlinux", work / "vmlinux-copy")
    if not notes.stat().st_size:
        raise ValueError("The release kernel has no ELF notes")
    return release, hashlib.sha256(notes.read_bytes()).hexdigest()


def payload_files(directory, blobs, prefix):
    if not directory.is_dir():
        raise ValueError(f"Driver payload is not a directory: {directory}")
    rows = []
    for path in sorted(directory.rglob("*")):
        if path.is_dir():
            if path.is_symlink():
                raise ValueError(f"Symbolic directory in driver payload: {path}")
            continue
        source = path.resolve(strict=True)
        name = path.relative_to(directory)
        if (not source.is_relative_to(directory) or not source.is_file()
                or name.parts[0] not in {"modules", "firmware", "graphics"}
                or len(name.parts) < 2
                or any(not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.+-]*", part) for part in name.parts)):
            raise ValueError(f"Invalid driver payload path: {path}")
        # Hash the copied bytes so the catalog describes exactly what is served.
        pending = blobs / ".incoming"
        shutil.copyfile(source, pending)
        with pending.open("rb") as data:
            size = data.seek(0, 2)
            data.seek(0)
            digest = hashlib.file_digest(data, "sha256").hexdigest()
        if not size:
            raise ValueError(f"Empty driver payload file: {path}")
        blob = blobs / (prefix + digest)
        if blob.exists():
            pending.unlink()
        else:
            pending.rename(blob)
        rows.append({"path": name.as_posix(), "sha256": digest, "size": size})
    if not rows or len(rows) > 256 or sum(row["size"] for row in rows) > 512 * 1024 * 1024:
        raise ValueError(f"Driver payload is empty or exceeds the installer limits: {directory}")
    return rows


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def validate_device(device):
    if not isinstance(device, dict):
        raise ValueError("Each device selector must be an object")
    bus = device.get("bus")
    if bus == "devicetree":
        compatible = device.get("compatible")
        if device.keys() == {"bus", "compatible"} and isinstance(compatible, str) and compatible.strip():
            return
    elif bus in ("pci", "usb", "sdio"):
        ids = {"vendor", "product"}
        fields = ids | {"bus"}
        if bus == "pci" and "subsystem" in device:
            subsystem = device["subsystem"]
            if (not isinstance(subsystem, dict) or subsystem.keys() != ids
                    or not all(type(value) is int and 0 <= value <= 0xffff for value in subsystem.values())):
                raise ValueError(f"Invalid PCI subsystem in selector: {device}")
            fields.add("subsystem")
        if device.keys() == fields and all(type(device[key]) is int and 0 <= device[key] <= 0xffff for key in ids):
            return
    elif bus == "modalias":
        pattern = device.get("pattern")
        if (device.keys() == {"bus", "pattern"} and isinstance(pattern, str)
                and re.fullmatch(r"(?:pci|usb):[a-zA-Z0-9*?\[\]-]+", pattern)):
            return
    raise ValueError(f"Invalid device selector: {device}")


def bundle_devices(bundle, directory):
    devices = bundle.get("devices", [])
    if not isinstance(devices, list):
        raise ValueError("Driver device selectors must be a list")
    for name in bundle.get("alias_modules", []):
        if not re.fullmatch(r"[a-zA-Z0-9_.+-]+\.ko", name):
            raise ValueError(f"Invalid alias module filename: {name}")
        module = (directory / "modules" / name).resolve(strict=True)
        if not module.is_relative_to(directory):
            raise ValueError(f"Alias module is outside its bundle: {name}")
        aliases = run("modinfo", "-F", "alias", module, capture_output=True, text=True).stdout
        devices.extend({"bus": "modalias", "pattern": pattern} for pattern in sorted(set(aliases.splitlines()))
                       if pattern.startswith(("pci:", "usb:")))
    if not devices:
        raise ValueError("Each driver bundle requires at least one device selector")
    for device in devices:
        validate_device(device)
    return devices


def prepare(args):
    url = urlsplit(args.url)
    if (url.scheme != "https" or not url.hostname or url.username is not None
            or any(char.isspace() or char in "?#%@" for char in args.url)):
        raise ValueError("Use an HTTPS catalog URL without credentials, query or fragment")
    manifest = args.manifest.resolve()
    spec = json.loads(manifest.read_text())
    if not spec.get("bundles"):
        raise ValueError("The manifest must describe at least one driver bundle")

    output = args.output.resolve()
    if output.exists():
        raise ValueError(f"Driver feed output already exists: {output}")

    with tempfile.TemporaryDirectory(prefix=".driver-feed-", dir=output.parent) as temp:
        work = Path(temp)
        release, build_hash = kernel_identity(args.kernel_build, args.objcopy, work)
        stage = work / "out"
        network = stage / "feed"
        config = stage / "image-config"
        release_assets = args.asset_layout == "release"
        blobs = network if release_assets else network / "files"
        prefix = "driver-" if release_assets else ""
        blobs.mkdir(parents=True)
        config.mkdir()
        bundles = []
        for bundle in spec["bundles"]:
            directory = (manifest.parent / bundle["payload"]).resolve()
            bundles.append({
                "arch": args.arch, "kernel": release, "kernel_build": build_hash,
                "devices": bundle_devices(bundle, directory),
                "files": payload_files(directory, blobs, prefix),
            })
        index = network / "index.json"
        write_json(index, {"version": 1, "bundles": bundles})
        if index.stat().st_size > 512 * 1024:
            raise ValueError("The signed index exceeds the installer limit")

        key = args.signing_key.resolve()
        run("ssh-keygen", "-Y", "sign", "-f", key, "-n", "angi-driver-catalog", index)
        public_key = run("ssh-keygen", "-y", "-f", key, capture_output=True, text=True).stdout
        (config / "driver-signers").write_text("angi-drivers " + public_key, encoding="utf-8")
        image_config = {"url": args.url.rstrip("/"), "kernel_build": build_hash}
        if release_assets:
            image_config["asset_layout"] = "release"
        write_json(config / "driver-catalog.json", image_config)
        stage.rename(output)
    print(f"Kernel {release} ({build_hash})")
    print(f"HTTPS payload: {output / 'feed'}")
    print(f"AGX_DRIVER_FEED_DIR={output / 'image-config'}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel-build", type=Path, required=True)
    parser.add_argument("--arch", choices=["x86_64", "aarch64", "arm"], required=True)
    parser.add_argument("--objcopy", default="objcopy")
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--signing-key", type=Path, required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--asset-layout", choices=["directory", "release"], default="directory")
    parser.add_argument("--output", type=Path, required=True)
    try:
        prepare(parser.parse_args())
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Driver feed preparation failed: {error}\n")


if __name__ == "__main__":
    main()
