#!/usr/bin/env python3
"""Prepare a private ANGI Pi SD image and its matching SSH trust file."""

import argparse
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile

from iso_media import sha256_file
from ssh_provisioning import checked_public_key, personalize_rootfs, workstation_public_key

ROOT = Path(__file__).resolve().parents[1]
IMAGE = "sdcard.img"
CHECKSUM = IMAGE + ".sha256"
HOSTS = {"rpi5": b"10.77.0.2,10.78.0.2", "rpizero2w": b"10.79.0.2"}


def board_for(directory, expected=None):
    board = (directory / "board.txt").read_text().strip()
    if board not in HOSTS:
        raise ValueError(f"Private SD provisioning is unavailable for {board}")
    if expected is not None and board != expected:
        raise ValueError(f"Image is for {board}, expected {expected}")
    return board


def verify_bundle(directory, expected=None):
    image = directory / IMAGE
    board = board_for(directory, expected)
    pins = (directory / "known_hosts").read_bytes().splitlines()
    if len(pins) != 1 or not pins[0].startswith(HOSTS[board] + b" ssh-ed25519 "):
        raise ValueError("The private install image is missing its Ed25519 SSH trust entry")
    subprocess.run(["ssh-keygen", "-lf", str(directory / "known_hosts")],
                   stdout=subprocess.DEVNULL, check=True)
    expected = (directory / CHECKSUM).read_text()
    if not re.fullmatch(r"[a-f0-9]{64}  sdcard\.img\n", expected):
        raise ValueError("Invalid private install image checksum")
    if sha256_file(image) != expected[:64]:
        raise ValueError("The private install image failed its SHA-256 check")


def provision(images, output_parent, public_key, dropbearkey):
    board = board_for(images)
    subprocess.run([sys.executable, str(ROOT / "scripts/check-image.py"), str(images),
                    "--board", board], check=True)
    keygen = shutil.which(dropbearkey)
    if keygen is None:
        raise ValueError("Install dropbearkey or supply its path with --dropbearkey")

    with tempfile.TemporaryDirectory(dir=output_parent, prefix=f".angi-{board}-provision-") as staging:
        staging = Path(staging)
        shutil.copyfile(images / "rootfs.ext4", staging / "rootfs")
        host_key = personalize_rootfs(staging, checked_public_key(public_key), str(Path(keygen).resolve()))
        shutil.copyfile(images / IMAGE, staging / IMAGE)
        with (staging / IMAGE).open("r+b") as disk, (staging / "rootfs").open("rb") as rootfs:
            # check-image verifies the two MBR entries and compares the base
            # disk's partition byte ranges against the Buildroot artifacts.
            disk.seek(462)
            entry = disk.read(16)
            start, sectors = struct.unpack_from("<II", entry, 8)
            if entry[4] != 0x83 or rootfs.seek(0, 2) != sectors * 512:
                raise ValueError("The personalized root filesystem does not fit the SD partition")
            rootfs.seek(0)
            disk.seek(start * 512)
            shutil.copyfileobj(rootfs, disk, 1024 * 1024)
            disk.seek(start * 512)
            rootfs.seek(0)
            while chunk := rootfs.read(1024 * 1024):
                if disk.read(len(chunk)) != chunk:
                    raise ValueError("The packaged root filesystem did not match its source")

        (staging / "known_hosts").write_bytes(HOSTS[board] + b" " + host_key)
        (staging / "board.txt").write_text(board + "\n")
        (staging / CHECKSUM).write_text(f"{sha256_file(staging / IMAGE)}  {IMAGE}\n")
        for artifact in (IMAGE, "known_hosts", "board.txt", CHECKSUM):
            (staging / artifact).chmod(0o600)
        verify_bundle(staging)

        install = Path(tempfile.mkdtemp(dir=output_parent, prefix=f"angi-{board}-install-"))
        for artifact in (IMAGE, "known_hosts", "board.txt", CHECKSUM):
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
    parser.add_argument("--board", choices=HOSTS, help="Expected target when verifying a private image")
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
