#!/usr/bin/env python3
"""Preflight and prepare the local artifacts for an ANGI source installation."""
import argparse
from pathlib import Path
import os
import platform
import shutil
import subprocess
import sys

from detect_board_registry import BY_ID, ROOT
from iso_media import sha256_file

FLASH = ROOT / "scripts/flash-sd.sh"
MIN_SPACE = 20 * 1024**3
TOOLS = ("git", "make", "gcc", "g++", "curl", "sha256sum", "tar", "xz", "cpio",
         "rsync", "unzip", "file", "bc", "patch", "ssh", "ssh-keygen", "dropbearkey",
         "debugfs", "lsblk", "sfdisk", "findmnt", "flock", "cmp", "wipefs", "blockdev")


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, text=True, **kwargs)


def preflight(board, device, interface, parent):
    run(sys.executable, "scripts/detect-board.py", "--require", board)
    config = BY_ID[board]
    if not (ROOT / config["install"]["first_boot"]).is_file():
        raise ValueError("This checkout lacks the first-boot guide")
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "aarch64"):
        raise ValueError("The source builder needs an x86-64 or ARM64 Linux environment")
    missing = [name for name in TOOLS if not shutil.which(name)]
    if device and not any(shutil.which(tool) for tool in ("udisksctl", "eject")):
        missing.append("udisksctl or eject")
    if interface:
        if "ethernet" not in config["install"]["media"]:
            raise ValueError(f"{board} does not support the Ethernet installer")
        missing.extend(tool for tool in ("ip", "dnsmasq", "readelf", "ssh-keyscan") if not shutil.which(tool))
    if missing:
        raise ValueError("Install the missing host tools: " + ", ".join(missing))
    if not parent.is_dir() or not os.access(parent, os.W_OK):
        raise ValueError("Choose an existing writable install directory outside the checkout")
    if parent.is_relative_to(ROOT):
        raise ValueError("Keep private installation artifacts outside the checkout")
    if shutil.disk_usage(ROOT).free < MIN_SPACE or shutil.disk_usage(parent).free < MIN_SPACE:
        raise ValueError("Allow at least 20 GiB of free space for the build and private artifacts")
    memory = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
    if memory < 2 * 1024**3:
        raise ValueError("Allocate at least 2 GiB of RAM to the Linux build environment")
    print(f"Linux {platform.machine()}, {memory / 1024**3:.1f} GiB RAM", flush=True)
    if device:
        run(FLASH, "--inspect", "--device", device)
    else:
        run(sys.executable, "scripts/net-install.py", "preflight", "--interface", interface)


def verify_artifact(images, board):
    name = BY_ID[board]["image"]
    if (images / "board.txt").read_text().strip() != board:
        raise ValueError("The build output has the wrong board stamp")
    image = images / name
    if (images / f"{name}.sha256").read_text() != f"{sha256_file(image)}  {name}\n":
        raise ValueError("The build output failed its SHA-256 check")


def prepare(board, device, interface, parent):
    commit = run("git", "rev-parse", "HEAD", capture_output=True).stdout.strip()
    if run("git", "status", "--porcelain", "--untracked-files=no",
           capture_output=True).stdout:
        raise ValueError("Prepare the install from an isolated, clean source checkout")
    print(f"Source commit: {commit}", flush=True)
    preflight(board, device, interface, parent)
    public_key = run(sys.executable, "scripts/ssh_provisioning.py", capture_output=True).stdout.strip()
    env = os.environ.copy()
    for option in ("AGX_BUILD_OUTPUT", "AGX_GENERIC_ISO"):
        env.pop(option, None)
    env["SSH_PUBLIC_KEY"] = public_key
    for step in ("configure", "dependencies", "build", "check-image"):
        run("make", step, f"BOARD={board}", env=env)
    images = ROOT / BY_ID[board]["output"] / "images"
    verify_artifact(images, board)
    run(sys.executable, "scripts/provision-sd.py", "--images", images, "--output-dir", parent,
        "--public-key", public_key)
    print(f"Workstation SSH identity: {Path(public_key).with_suffix('')}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("preflight", "prepare"))
    parser.add_argument("--board", required=True, choices=BY_ID)
    transport = parser.add_mutually_exclusive_group(required=True)
    transport.add_argument("--device", action="append", help="whole removable device visible to Linux")
    transport.add_argument("--interface", action="append", help="spare Ethernet interface for network boot")
    parser.add_argument("--output-dir", type=Path, required=True, help="private parent outside the checkout")
    args = parser.parse_args()
    selected = args.device or args.interface
    if len(selected) != 1:
        parser.error("Select exactly one device or interface before preparing the installation")
    operation = preflight if args.action == "preflight" else prepare
    try:
        operation(args.board, selected[0] if args.device else None,
                  selected[0] if args.interface else None, args.output_dir.resolve())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Source install stopped: {error}")


if __name__ == "__main__":
    main()
