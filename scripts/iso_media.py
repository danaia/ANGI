#!/usr/bin/env python3
"""Build and verify ANGI Pi 4 / 400 ISO media before it reaches a block device."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile


MANIFEST = "ANGI-MEDIA.JSON"
BOARD = "rpi4"
PARTITIONS = (("boot", 0x0C), ("rootfs", 0x83))
CHUNK_SIZE = 1024 * 1024


def partition_regions(image):
    """Read the FAT and ext4 partition locations from the hybrid ISO's MBR."""
    size = image.stat().st_size
    with image.open("rb") as stream:
        stream.seek(16 * 2048)
        if stream.read(7) != b"\x01CD001\x01":
            raise ValueError("Missing ISO 9660 primary volume descriptor")
        stream.seek(0)
        mbr = stream.read(512)
    if mbr[510:512] != b"\x55\xaa" or any(mbr[478:510]):
        raise ValueError("Expected an MBR with two Pi partitions")
    end = 17 * 2048
    regions = {}
    for index, (name, kind) in enumerate(PARTITIONS):
        entry = mbr[446 + 16 * index:462 + 16 * index]
        if entry[4] != kind or entry[0] != (0x80 if index == 0 else 0):
            raise ValueError(f"Unexpected MBR entry for {name}")
        start, count = struct.unpack_from("<II", entry, 8)
        offset, length = start * 512, count * 512
        if offset < end or length == 0 or offset + length > size:
            raise ValueError(f"Invalid partition bounds for {name}")
        regions[name] = (offset, length)
        end = offset + length
    return regions


def partition_chunks(stream, length):
    while length:
        chunk = stream.read(min(length, CHUNK_SIZE))
        if not chunk:
            raise ValueError("Truncated partition data")
        length -= len(chunk)
        yield chunk


def verify_iso(image, edition=None, destination=None):
    """Verify both partition hashes and optionally extract them to scratch files."""
    regions = partition_regions(image)
    manifest = json.loads(subprocess.check_output(["bsdtar", "-xOf", str(image), MANIFEST]))
    if not isinstance(manifest, dict) or manifest.get("board") != BOARD or manifest.get("version") != 1:
        raise ValueError("Expected an ANGI Pi 4 / 400 ISO")
    actual_edition = manifest.get("edition")
    if actual_edition not in ("generic", "personalized") or edition not in (None, actual_edition):
        raise ValueError(f"Expected a {edition or 'valid'} ANGI ISO, found {actual_edition}")
    hashes = manifest.get("partition_sha256")
    if not isinstance(hashes, dict):
        raise ValueError("Missing ISO partition checksums")
    with image.open("rb") as stream:
        for name, (offset, length) in regions.items():
            stream.seek(offset)
            digest = hashlib.sha256()
            if destination is None:
                for chunk in partition_chunks(stream, length):
                    digest.update(chunk)
            else:
                with (destination / name).open("xb") as extracted:
                    for chunk in partition_chunks(stream, length):
                        digest.update(chunk)
                        extracted.write(chunk)
            if digest.hexdigest() != hashes.get(name):
                raise ValueError(f"ISO partition checksum failed for {name}")


def sha256_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build_iso(boot, rootfs, output, edition, xorriso=None):
    paths = {"boot": boot, "rootfs": rootfs}
    for path in paths.values():
        if path.stat().st_size % 2048:
            raise ValueError(f"Partition image is not sector-aligned: {path.name}")
    xorriso = xorriso or os.environ.get("XORRISO") or "xorriso"
    if not shutil.which(xorriso):
        raise ValueError("Install xorriso or set XORRISO to a built copy")
    with tempfile.TemporaryDirectory(dir=output.parent, prefix=".angi-iso-") as temporary:
        staging = Path(temporary)
        files = staging / "iso"
        files.mkdir()
        manifest = {
            "version": 1,
            "board": BOARD,
            "edition": edition,
            "partition_sha256": {name: sha256_file(path) for name, path in paths.items()},
        }
        (files / MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")
        instructions = (
            "Write this generic ISO to a card for USB-C browser setup, or use scripts/provision-iso.py for SSH setup."
            if edition == "generic" else "This ISO contains a device-specific SSH host key. Keep it private."
        )
        (files / "README.TXT").write_text(f"ANGI for Raspberry Pi 4 / 400\n\n{instructions}\n")
        image = staging / output.name
        subprocess.run([
            xorriso, "-as", "mkisofs", "-R", "-V", "ANGI_RPI4", "--mbr-force-bootable",
            "-append_partition", "1", "0x0c", str(boot),
            "-append_partition", "2", "0x83", str(rootfs),
            "-o", str(image), str(files),
        ], check=True)
        # Keep the Pi 4 root PARTUUID when xorriso creates the hybrid MBR.
        with image.open("r+b") as stream:
            stream.seek(440)
            stream.write(struct.pack("<I", 0x41475804))
        verify_iso(image, edition)
        image.chmod(0o644 if edition == "generic" else 0o600)
        image.replace(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("--edition", choices=("generic", "personalized"))
    options = parser.parse_args()
    try:
        verify_iso(options.image, options.edition)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"ISO validation failed: {error}")
