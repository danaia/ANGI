"""Kernel identity shared by driver feed signing and image packaging."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import urlsplit


def kernel_identity(build, objcopy):
    release = (build / "include/config/kernel.release").read_text().strip()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.+-]*", release):
        raise ValueError("Invalid kernel release")
    with tempfile.TemporaryDirectory(prefix="angi-kernel-notes-") as temp:
        notes = Path(temp) / "kernel.notes"
        subprocess.run([objcopy, "-O", "binary", "--only-section=.notes",
                        build / "vmlinux", notes], check=True)
        data = notes.read_bytes()
    if not data:
        raise ValueError("The release kernel has no ELF notes")
    return release, hashlib.sha256(data).hexdigest()


def validate_feed_url(url):
    parts = urlsplit(url)
    if (parts.scheme != "https" or not parts.hostname
            or any(char.isspace() or char in "?#%@" for char in url)):
        raise ValueError("Use an HTTPS catalog URL without credentials, query or fragment")


def check_image_config(directory, kernel_build, objcopy):
    config = json.loads((directory / "driver-catalog.json").read_text())
    validate_feed_url(config["url"])
    if config.get("asset_layout", "directory") not in {"directory", "release"}:
        raise ValueError("Unsupported driver asset layout")
    if not (directory / "driver-signers").read_text().strip():
        raise ValueError("The driver feed needs a release public key")
    release, digest = kernel_identity(kernel_build, objcopy)
    if config.get("kernel_build") != digest:
        raise ValueError(f"The driver feed does not match kernel {release} ({digest})")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("kernel_build", type=Path)
    parser.add_argument("--objcopy", default="objcopy")
    args = parser.parse_args()
    try:
        check_image_config(args.directory, args.kernel_build, args.objcopy)
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Invalid ANGI driver feed configuration: {error}\n")
