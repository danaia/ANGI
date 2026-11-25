#!/usr/bin/env python3
"""Verify the hosted copy of a signed ANGI driver release before shipping an image."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def verify(release, ca_file):
    config = release / "image-config"
    base = json.loads((config / "driver-catalog.json").read_text())
    url = base["url"].rstrip("/")
    asset_layout = base.get("asset_layout", "directory")
    if asset_layout not in ("directory", "release"):
        raise ValueError(f"Unsupported driver asset layout: {asset_layout}")
    if not url.startswith("https://"):
        raise ValueError("Driver feeds require an HTTPS URL")
    prefix = "driver-" if asset_layout == "release" else "files/"
    local_index = (release / "feed" / "index.json").read_bytes()
    curl = [
        "curl", "-q", "--fail", "--silent", "--show-error", "--location",
        "--proto", "=https", "--proto-redir", "=https", "--max-redirs", "5",
        "--connect-timeout", "10", "--max-time", "240",
    ]
    if ca_file:
        curl.extend(["--cacert", str(ca_file)])

    with tempfile.TemporaryDirectory(prefix="angi-driver-feed-") as work:
        def fetch(name, limit):
            path = Path(work) / Path(name).name
            subprocess.run([
                *curl, "--max-filesize", str(limit), "--output", str(path), "--", f"{url}/{name}",
            ], check=True)
            return path

        index = fetch("index.json", 512 * 1024)
        signature = fetch("index.json.sig", 16 * 1024)
        with index.open("rb") as data:
            subprocess.run([
                "ssh-keygen", "-Y", "verify", "-f", config / "driver-signers",
                "-I", "angi-drivers", "-n", "angi-driver-catalog", "-s", signature,
            ], stdin=data, check=True)
        if local_index != index.read_bytes():
            raise ValueError("Hosted catalog differs from the release catalog")
        bundles = json.loads(local_index)["bundles"]
        if any(bundle["kernel_build"] != base["kernel_build"] for bundle in bundles):
            raise ValueError("A driver bundle targets a different kernel build")
        blobs = {item["sha256"]: item["size"] for bundle in bundles for item in bundle["files"]}
        for digest, size in blobs.items():
            path = fetch(f"{prefix}{digest}", size)
            with path.open("rb") as file:
                actual = hashlib.file_digest(file, "sha256").hexdigest()
            if path.stat().st_size != size or actual != digest:
                raise ValueError(f"Hosted driver failed verification: {digest}")
            path.unlink()
    print(f"Verified signed catalog and {len(blobs)} driver files at {url}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("release", type=Path, help="Output directory from build-driver-feed.py")
    parser.add_argument("--ca-file", type=Path, help="Trust the specified CA for a private test feed")
    args = parser.parse_args()
    try:
        verify(args.release, args.ca_file)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Driver feed check failed: {error}\n")
