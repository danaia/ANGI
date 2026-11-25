#!/usr/bin/env python3
"""Run a direct-Ethernet ANGI bootstrap and approved, verified disk transfer."""
import argparse
import os
import json
from pathlib import Path
import re
import shlex
import socket
import subprocess
import sys

from detect_board_registry import BY_ID, ROOT
from net_bootstrap import CLIENT, HOST, make_bootstrap

BOARDS = ("rpi4", "rpi5", "arm-desktop", "x86-desktop", "x86-amd")


def command(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def ip_json(*args):
    return json.loads(command("ip", "-j", *args, capture_output=True).stdout)


def dedicated(interface, configured=False):
    socket.if_nametoindex(interface)
    if not Path("/sys/class/net", interface, "device").exists():
        # A PCI or USB adapter assigned to a local VM also has a device entry.
        raise ValueError("Choose a dedicated physical or guest Ethernet adapter")
    if Path("/sys/class/net", interface, "wireless").exists():
        raise ValueError("Choose a wired Ethernet adapter")
    for family in ("-4", "-6"):
        if ip_json(family, "route", "show", "default", "dev", interface):
            raise ValueError("The selected adapter carries a default route")
    link, = ip_json("address", "show", "dev", interface)
    addresses = [(item["local"], item["prefixlen"]) for item in link["addr_info"]
                 if item["scope"] == "global"]
    if addresses not in ([], [(HOST, 24)]) or (configured and not addresses):
        raise ValueError(f"Use a spare adapter configured only as {HOST}/24, with no gateway")


def fingerprint_scan(fingerprint, trust):
    if not re.fullmatch(r"SHA256:[A-Za-z0-9+/]{43}", fingerprint):
        raise ValueError("Use the SHA256 fingerprint displayed on the target console")
    scan = command("ssh-keyscan", "-T", "5", "-t", "ed25519", CLIENT,
                   capture_output=True).stdout.strip()
    lines = scan.splitlines()
    if len(lines) != 1 or not lines[0].startswith(f"{CLIENT} ssh-ed25519 "):
        raise ValueError("The installer did not present one Ed25519 host key")
    presented = command("ssh-keygen", "-l", "-E", "sha256", "-f", "-",
                        input=scan + "\n", capture_output=True).stdout.split()[1]
    if presented != fingerprint:
        raise ValueError("The installer key differs from the target console fingerprint")
    fd = os.open(trust, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as pins:
        pins.write(scan + "\n")


def require_route(interface):
    dedicated(interface, configured=True)
    route, = ip_json("route", "get", CLIENT)
    if route.get("dev") != interface or route.get("prefsrc") != HOST:
        raise ValueError("The installer route does not use the dedicated adapter")


def ssh(interface, key, trust, *args, **kwargs):
    require_route(interface)
    command("ssh", "-F", "/dev/null", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8",
            "-o", "StrictHostKeyChecking=yes", "-o", f"UserKnownHostsFile={trust}",
            "-o", "GlobalKnownHostsFile=/dev/null", "-o", "IdentitiesOnly=yes",
            "-i", key, f"root@{CLIENT}", shlex.join(args), **kwargs)


def verify_payload(board, bundle):
    command(sys.executable, ROOT / "scripts/provision-sd.py", "--verify", bundle, "--board", board)
    return bundle / BY_ID[board]["image"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    for action in ("preflight", "stage", "serve", "pin", "identity", "list", "inspect", "write"):
        sub = actions.add_parser(action)
        if action != "stage":
            sub.add_argument("--interface", required=True)
        if action in ("stage", "serve", "write"):
            sub.add_argument("--board", required=True, choices=BOARDS)
        if action in ("stage", "write"):
            sub.add_argument("--bundle", type=Path, required=True)
        if action in ("stage", "serve"):
            sub.add_argument("--boot-dir", type=Path, required=True)
        if action == "stage":
            sub.add_argument("--public-key", type=Path, required=True)
        if action in ("pin", "identity", "list", "inspect", "write"):
            sub.add_argument("--known-hosts", type=Path, required=True)
        if action == "pin":
            sub.add_argument("--fingerprint", required=True)
        if action in ("identity", "list", "inspect", "write"):
            sub.add_argument("--key", type=Path, required=True)
        if action in ("inspect", "write"):
            sub.add_argument("--device", required=True)
        if action == "write":
            sub.add_argument("--confirm-erase", required=True)
            sub.add_argument("--expect-identity", required=True)
    args = parser.parse_args()
    action = args.action
    if action == "preflight":
        dedicated(args.interface)
    elif action == "stage":
        verify_payload(args.board, args.bundle)
        args.boot_dir.mkdir(mode=0o755)
        make_bootstrap(args.board, args.public_key, args.boot_dir)
        # This directory contains the public bootstrap only. TFTP needs read access.
        args.boot_dir.chmod(0o755)
        for artifact in args.boot_dir.iterdir():
            artifact.chmod(0o644)
    elif action == "serve":
        dedicated(args.interface, configured=True)
        marker = "config.txt" if args.board.startswith("rpi") else "bootstrap.efi"
        if not (args.boot_dir / marker).is_file() or not (args.boot_dir / "installer.cpio.gz").is_file():
            raise ValueError("Stage the bootstrap before starting the isolated server")
        options = (["--pxe-service=0,Raspberry Pi Boot,bootcode.bin"] if args.board.startswith("rpi")
                   else ["--dhcp-boot=bootstrap.efi"])
        command("dnsmasq", "--no-daemon", "--conf-file=/dev/null", "--port=0", "--bind-interfaces",
                "--except-interface=lo", f"--interface={args.interface}", f"--listen-address={HOST}",
                f"--dhcp-range={CLIENT},{CLIENT},255.255.255.0,2m",
                "--dhcp-option=3", "--dhcp-option=6", "--leasefile-ro", "--enable-tftp",
                f"--tftp-root={args.boot_dir.resolve()}", *options)
    elif action == "pin":
        require_route(args.interface)
        fingerprint_scan(args.fingerprint, args.known_hosts)
    else:
        connection = (args.interface, args.key, args.known_hosts)
        if action == "identity":
            ssh(*connection, "sh", "-c",
                "uname -m; cat /proc/device-tree/model /sys/class/dmi/id/product_name 2>/dev/null; "
                "grep -m1 vendor_id /proc/cpuinfo || true; grep -w angi.netinstall=1 /proc/cmdline")
        elif action == "list":
            ssh(*connection, "angi-install-disk", "list")
        elif action == "inspect":
            ssh(*connection, "angi-install-disk", "inspect", args.device)
        else:
            if args.device != args.confirm_erase:
                raise ValueError("Erase approval must repeat the exact device path")
            image = verify_payload(args.board, args.bundle)
            digest = (args.bundle / f"{image.name}.sha256").read_text().split()[0]
            with image.open("rb") as payload:
                ssh(*connection, "angi-install-disk", "write", args.device, args.confirm_erase,
                    args.expect_identity, str(os.fstat(payload.fileno()).st_size), digest, stdin=payload)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Network installation stopped: {error}")
