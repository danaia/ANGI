#!/usr/bin/env python3
"""Prepare a private ANGI Pi SD image and its matching SSH trust file."""

import argparse
import hmac
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile

from detect_board_registry import BY_ID
from iso_media import sha256_file, partition_chunks
from uefi_image import partitions
from ssh_provisioning import checked_public_key, personalize_rootfs, workstation_public_key

ROOT = Path(__file__).resolve().parents[1]
LEGACY_HOSTS = {"rpi5": b"10.77.0.2,10.78.0.2", "rpizero2w": b"10.79.0.2"}
RECEIPT = "install.json"


def board_for(directory, expected=None):
    board = (directory / "board.txt").read_text().strip()
    if board not in BY_ID or "install" not in BY_ID[board]:
        raise ValueError(f"Private media provisioning is unavailable for {board}")
    if expected is not None and board != expected:
        raise ValueError(f"Media is for {board}, expected {expected}")
    return board


def host_names(board):
    entry = BY_ID[board]
    addresses = [link["pi"] for link in entry["links"].values() if link]
    return ",".join([entry["install"]["ssh_host_alias"], *addresses]).encode()


def verify_bundle(directory, expected=None):
    board = board_for(directory, expected)
    image = directory / BY_ID[board]["image"]
    trust = directory / "known_hosts"
    pins = trust.read_bytes().splitlines()
    receipt = directory / RECEIPT
    names = host_names(board) if receipt.exists() else LEGACY_HOSTS.get(board)
    if names is None or len(pins) != 1 or not pins[0].startswith(names + b" ssh-ed25519 "):
        raise ValueError("The private install bundle is missing its Ed25519 SSH trust entry")
    subprocess.run(["ssh-keygen", "-lf", str(trust)], stdout=subprocess.DEVNULL, check=True)
    checksum = (directory / f"{image.name}.sha256").read_text()
    if not re.fullmatch(rf"[a-f0-9]{{64}}  {re.escape(image.name)}\n", checksum):
        raise ValueError("Invalid private install artifact checksum")
    digest = sha256_file(image)
    if not hmac.compare_digest(digest, checksum[:64]):
        raise ValueError("The private install artifact failed its SHA-256 check")
    if receipt.exists():
        expected_receipt = {"board": board, "image_sha256": digest,
                            "known_hosts_sha256": sha256_file(trust)}
        if json.loads(receipt.read_text()) != expected_receipt:
            raise ValueError("The media and SSH trust record do not match the install receipt")


def root_region(image, board):
    if board.endswith("-desktop"):
        root = partitions(image)[1]
        return root["start"] * 512, root["size"] * 512
    with image.open("rb") as disk:
        disk.seek(462)
        entry = disk.read(16)
    if len(entry) != 16 or entry[4] != 0x83:
        raise ValueError("Expected a Linux root partition")
    start, sectors = struct.unpack_from("<II", entry, 8)
    return start * 512, sectors * 512


def pack_root(image, rootfs, board):
    offset, length = root_region(image, board)
    if length != rootfs.stat().st_size:
        raise ValueError("The personalized root filesystem does not fit the root partition")
    with image.open("r+b") as disk, rootfs.open("rb") as source:
        disk.seek(offset)
        shutil.copyfileobj(source, disk, 1024 * 1024)
        disk.seek(offset)
        with rootfs.open("rb") as reference:
            if any(reference.read(len(chunk)) != chunk for chunk in partition_chunks(disk, length)):
                raise ValueError("The packaged root filesystem did not match its source")


def provision(images, output_parent, public_key, dropbearkey):
    board = board_for(images)
    name = BY_ID[board]["image"]
    subprocess.run([sys.executable, str(ROOT / "scripts/check-image.py"), str(images),
                    "--board", board], check=True)
    source_image = images / name
    source_checksum = (images / f"{name}.sha256").read_text()
    if source_checksum != f"{sha256_file(source_image)}  {name}\n":
        raise ValueError("The verified build is missing its matching artifact checksum")
    keygen = shutil.which(dropbearkey)
    if keygen is None:
        raise ValueError("Install dropbearkey or supply its path with --dropbearkey")

    with tempfile.TemporaryDirectory(dir=output_parent, prefix=f".angi-{board}-provision-") as temp:
        staging = Path(temp)
        shutil.copyfile(images / "rootfs.ext4", staging / "rootfs")
        host_key = personalize_rootfs(staging, checked_public_key(public_key), str(Path(keygen).resolve()))
        image = staging / name
        shutil.copyfile(source_image, image)
        pack_root(image, staging / "rootfs", board)
        trust = staging / "known_hosts"
        trust.write_bytes(host_names(board) + b" " + host_key)
        (staging / "board.txt").write_text(f"{board}\n")
        digest = sha256_file(image)
        checksum = f"{name}.sha256"
        (staging / checksum).write_text(f"{digest}  {name}\n")
        (staging / RECEIPT).write_text(json.dumps(
            {"board": board, "image_sha256": digest, "known_hosts_sha256": sha256_file(trust)},
            indent=2) + "\n")
        artifacts = (name, "known_hosts", "board.txt", checksum, RECEIPT)
        for artifact in artifacts:
            (staging / artifact).chmod(0o600)
        verify_bundle(staging, board)

        install = Path(tempfile.mkdtemp(dir=output_parent, prefix=f"angi-{board}-install-"))
        for artifact in artifacts:
            (staging / artifact).replace(install / artifact)

    print(f"Install directory: {install}", flush=True)
    subprocess.run(["ssh-keygen", "-lf", str(install / "known_hosts")], check=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--images", type=Path, default=ROOT / "output/images")
    parser.add_argument("--public-key", type=Path, help="OpenSSH public key (default: prepare a local Ed25519 pair)")
    parser.add_argument("--output-dir", type=Path, default=Path.cwd())
    parser.add_argument("--dropbearkey", default="dropbearkey")
    parser.add_argument("--verify", type=Path, metavar="INSTALL_DIRECTORY")
    parser.add_argument("--board", choices=BY_ID, help="Expected target when verifying a private image")
    args = parser.parse_args()
    if args.verify:
        verify_bundle(args.verify, args.board)
    else:
        if args.board:
            parser.error("--board requires --verify; provisioning uses the build's board stamp")
        public = args.public_key if args.public_key is not None else workstation_public_key()
        provision(args.images.resolve(strict=True), args.output_dir.resolve(strict=True),
                  public, args.dropbearkey)
        if args.public_key is None:
            print(f"Workstation SSH identity: {public.with_suffix('')}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Could not prepare or verify the ANGI SD image: {error}")
