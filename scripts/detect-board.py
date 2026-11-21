#!/usr/bin/env python3
"""Work out which ANGI target is being installed and whether an image exists for it.

  detect-board.py --list                    show every target and its status
  detect-board.py --host                    what this computer is (arch, OS)
  detect-board.py --probe-usb               look for a Pi that is visible over USB right now
  detect-board.py --identify HOST [--key K] read the board model from a running Pi over SSH
  detect-board.py --require ID              exit 0 only if ID has an image (ready or experimental); else exit 3
  detect-board.py --field ID KEY            print one registry value (for build scripts)

A Pi with an empty card slot usually cannot be identified from the computer: Pi 4 and Pi 5
do not appear on USB without an OS. Only the Zero 2 W's boot ROM does. So the installer must
ask the person (use the how_to_tell text) and then confirm after the first boot.
"""
import argparse, json, platform, re, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from detect_board_registry import BOARDS, BY_ID, ROOT

READY = [b["id"] for b in BOARDS if b["status"] in ("ready", "experimental")]


def match_model(model):
    for b in BOARDS:
        if b["dt_model"] and re.search(b["dt_model"], model):
            return b
    return None


def cmd_list(_):
    for b in BOARDS:
        print(f'{b["id"]:12} {b["status"]:8} {b["name"]}  [{b["medium"]}]')
        print(f'{"":21}{b["how_to_tell"]}')


def cmd_host(_):
    machine = platform.machine()
    info = {"arch": machine, "os": platform.platform(),
            "efi": Path("/sys/firmware/efi").exists(),
            "target_if_this_is_the_target": "x86-desktop" if machine == "x86_64" else
                                            "arm-desktop" if machine in ("aarch64", "arm64") else "unknown"}
    print(json.dumps(info, indent=2))


def cmd_probe_usb(_):
    out = subprocess.run(["lsusb"], capture_output=True, text=True).stdout
    found = []
    for b in BOARDS:
        for ident in b["usb_boot_ids"]:
            if ident in out:
                found.append((b["id"], f"{ident} (USB boot mode: no card or no bootable card)"))
    gadget = subprocess.run(["ip", "-br", "link"], capture_output=True, text=True).stdout
    if "02:77:00:00:00:02" in subprocess.run(["ip", "link"], capture_output=True, text=True).stdout:
        found.append(("angi", "an ANGI USB-C Ethernet adapter (MAC 02:77:00:00:00:02) is up"))
    if not found:
        print("No Pi visible over USB. That is normal with an empty card slot (Pi 4 and Pi 5 stay silent).")
        return 1
    for bid, why in found:
        print(f"{bid}: {why}")


def cmd_identify(a):
    opts = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "StrictHostKeyChecking=yes"]
    if a.key:
        opts += ["-i", a.key, "-o", "IdentitiesOnly=yes"]
    r = subprocess.run(["ssh", *opts, a.identify, "cat /proc/device-tree/model; echo; uname -m"],
                       capture_output=True, text=True)
    if r.returncode:
        print("Could not read the board:", r.stderr.strip(), file=sys.stderr)
        return 2
    model, arch = (r.stdout.strip().split("\n") + [""])[:2]
    b = match_model(model.replace("\x00", ""))
    print(json.dumps({"model": model.replace("\x00", ""), "arch": arch,
                      "target": b["id"] if b else None,
                      "status": b["status"] if b else "unknown"}, indent=2))


def cmd_require(a):
    b = BY_ID.get(a.require)
    if not b:
        print(f"Unknown target '{a.require}'. Known: {', '.join(BY_ID)}", file=sys.stderr)
        return 2
    if b["status"] in ("ready", "experimental"):
        config = ROOT / "buildroot/configs" / b["defconfig"]
        if not config.is_file():
            print(f"Checkout lacks the {b['id']} defconfig: {config}", file=sys.stderr)
            return 3
        print(f'{b["id"]}: {b["status"]} ({b.get("defconfig")})')
        if b["status"] == "experimental":
            print(f'NOTE: {b.get("note", "not yet booted on hardware")}', file=sys.stderr)
        return 0
    print(f'{b["name"]} is not supported yet: there is no ANGI image for it.\n'
          f'Do not flash another board\'s image. Ready targets: {", ".join(READY)}.\n'
          f'Starting point for a port: Buildroot {b.get("base_buildroot_defconfig", "a UEFI/disk installer")}.',
          file=sys.stderr)
    return 3


def cmd_field(a):
    b = BY_ID.get(a.field[0])
    if not b or a.field[1] not in b:
        print(f"No value '{a.field[1]}' for '{a.field[0]}'", file=sys.stderr)
        return 2
    v = b[a.field[1]]
    print(" ".join(v) if isinstance(v, list) else v)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--list", action="store_true")
    p.add_argument("--host", action="store_true")
    p.add_argument("--probe-usb", action="store_true")
    p.add_argument("--identify", metavar="USER@HOST")
    p.add_argument("--key")
    p.add_argument("--require", metavar="ID")
    p.add_argument("--field", nargs=2, metavar=("ID", "KEY"))
    a = p.parse_args()
    for flag, fn in (("list", cmd_list), ("host", cmd_host), ("probe_usb", cmd_probe_usb),
                     ("identify", cmd_identify), ("require", cmd_require), ("field", cmd_field)):
        if getattr(a, flag):
            return fn(a) or 0
    p.print_help()
    return 2


sys.exit(main())
