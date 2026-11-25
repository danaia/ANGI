#!/usr/bin/env python3
"""Generate the browser prompt profiles, README prompt and Pi first-boot guides."""
import argparse
import json
import re
import subprocess

from detect_board_registry import ROOT, published_boards

DOCS = ROOT / "docs/install-prompts"
PIN = ROOT / "website/source-commit.txt"

MEDIA = {
    "sd": "microSD card in the Pi's onboard slot",
    "usb": "USB stick or SD card in a USB reader",
    "ethernet": "direct Ethernet network installation",
}
MEDIA_ACCESS = """Use scripts/flash-sd.sh --list to find the whole media device. If more than
one device could be mine, stop and ask me to identify it before continuing."""
MEDIA_DEPLOY = """Run scripts/flash-sd.sh --inspect --device PATH to show me the whole path, size,
model, serial or stable identity, and contents. Wait for me to type erase PATH
with that exact path. Reinspect immediately before writing. If the identity or
contents changed, stop for new approval. Run:
  scripts/flash-sd.sh --board {board} --image PRIVATE_DIR/{image} --device PATH --personalized --expect-identity IDENTITY --confirm-erase PATH
Use the Identity line from inspection. Wait for successful byte read-back and
ejection before telling me to remove the media. With the target powered off,
move it into the selected slot or USB port. For a USB reader or stick, select it
in the target's USB/external-media boot menu if needed. Power it and wait for me
to say Continue. The target runs from this media. Preserve its internal disks."""
ETHER_ACCESS = """Confirm there is a dedicated spare Ethernet interface for the installer. Keep
the host's default route on its existing connection."""
ETHER_DEPLOY = """Use the repository's network installer guide at docs/install-prompts/ethernet.md.
Finish staging before asking me to connect the cable, choose network boot and
say Continue. Verify the temporary installer's host fingerprint from the target
console and identify the running hardware. Show the exact target disk; obtain
erase approval for it and verify the written bytes before booting the result."""
PI_LINKS = """Use either a USB-C data cable with adequate power, or direct Ethernet with the
Pi's own supply. On USB-C, match adapter MAC 02:77:00:00:00:01 and set the host
to 10.77.0.1/24 for the Pi at 10.77.0.2. On a spare wired port, use
10.78.0.1/24 for the Pi at 10.78.0.2. Set no gateway and preserve the host's
internet route."""
ZERO_LINK = """Use a data cable in the micro-USB port marked USB. For additional power use
PWR IN. Match host adapter MAC 02:79:00:00:00:01, use 10.79.0.1/24 without a
gateway and connect to 10.79.0.2. See docs/pi-zero-2w.md."""
PEBBLE_SETUP = """Open the Pebble first-boot wizard on the target's display. For remote setup,
forward a free workstation loopback port to the target's 127.0.0.1:5900 over the
verified SSH connection. Check for an RFB greeting and open the local port in
a VNC viewer. Use the same wizard over VNC on headless images.

Follow Connect to Wi-Fi, Pair Bluetooth devices, and Create your administrator
account. Wi-Fi can be deferred, and Bluetooth pairing is optional. Have the
person enter passwords directly in the wizard. After account setup, confirm the sign-in
screen appears. For Wi-Fi, check agx wifi status --json and agx wifi check.
If the wizard cannot be reached, use agx tui over trusted SSH for recovery and
report which setup path was used."""
TERMINAL_SETUP = """Use agx tui over the verified SSH connection on this image. Follow the guide
to set up the administrator and network, with passwords entered in the terminal."""
NATIVE_BOOT = """Choose Native on the ANGI sign-in screen. Use a local display or connect
through the SSH-forwarded VNC session described in the guide. After sign-in,
open a built-in panel and verify the running agx-gui uses a LinuxKMS or software
VNC backend, as appropriate. Report the display path you tested."""
FULL_BOOT = """For Full Desktop, use a local HDMI display and keyboard with any VNC viewer
disconnected. If already signed in to Native, sign out first. Verify Weston is
installed and seatd is running with /run/seatd.sock. Choose Full Desktop at
sign-in and verify Weston is running and the ANGI shell is connected to Wayland.
For headless setup, report the Full Desktop check as pending. If sign-in shows
another reason the session is unavailable, report it."""
HEADLESS_BOOT = """Use the SSH-forwarded VNC connection from the guide. Choose Native at
sign-in, open a built-in panel, and verify the software VNC backend is running."""
TERMINAL_BOOT = """Verify the target reaches a text login on its local console and that agx tui
opens over the authenticated SSH connection. Report the console and SSH checks."""


def image_desktop(board, source):
    # Read the packages from the exact checkout the copied prompt will build.
    path = f"buildroot/configs/{board['defconfig']}"
    config = set(subprocess.check_output(
        ["git", "show", f"{source}:{path}"], cwd=ROOT, text=True).splitlines())
    if "BR2_PACKAGE_AGX_GUI=y" not in config:
        return "This image includes the ANGI terminal dashboard and a text console.", TERMINAL_SETUP, TERMINAL_BOOT
    if "BR2_PACKAGE_AGX_GUI_PROVISION_GRAPHICS=y" in config:
        setup = ("Use the local setup. Connect to Wi-Fi or Ethernet, then choose Prepare drivers. "
                 "If setup installs a driver bundle, restart and let setup check the drivers again "
                 "before creating the administrator account.")
        return "This image includes the Native Slint desktop.", setup, NATIVE_BOOT
    if "BR2_PACKAGE_AGX_GUI_HEADLESS=y" in config:
        return "This image includes the Native Slint desktop over VNC.", PEBBLE_SETUP, HEADLESS_BOOT
    if "BR2_PACKAGE_AGX_WAYLAND=y" in config:
        return (
            "This image includes Native (lightweight Slint) and Full Desktop (Wayland with Weston). "
            "Choose a session at sign-in.",
            PEBBLE_SETUP,
            NATIVE_BOOT + "\n\n" + FULL_BOOT,
        )
    return "This image includes the Native Slint desktop.", PEBBLE_SETUP, NATIVE_BOOT


def profiles(source):
    result = {}
    for board in published_boards(source):
        install = board["install"]
        desktop_summary, setup, desktop_checks = image_desktop(board, source)
        for choice in install["choices"]:
            modes = {}
            for mode in install["media"]:
                ethernet = mode == "ethernet"
                modes[mode] = {
                    "label": MEDIA[mode],
                    "experimental": mode == "ethernet" or (mode == "usb" and board["id"].startswith("rpi")),
                    "access": ETHER_ACCESS if ethernet else MEDIA_ACCESS,
                    "prepareArgs": ("--interface IFACE" if ethernet else "--device PATH")
                                   + " --output-dir PRIVATE_PARENT",
                    "deploy": ETHER_DEPLOY if ethernet else MEDIA_DEPLOY.format(
                        board=board["id"], image=board["image"]),
                }
            identity = "Read /proc/device-tree/model and run agx status --json."
            if board["id"].endswith("-desktop") or board["id"] == "x86-amd":
                identity = f"Check uname -m equals {board['arch']}, confirm the model and UEFI boot, and run agx status --json."
                if "cpu_vendor" in choice:
                    identity += f" Check /proc/cpuinfo vendor_id equals {choice['cpu_vendor']}."
            result[choice["id"]] = {
                **choice, "board": board["id"], "artifact": board["image"], "status": board["status"],
                "guide": install["first_boot"], "connection": install["connection"],
                "alias": install["ssh_host_alias"], "identity": identity, "modes": modes,
                "desktopSummary": desktop_summary, "setup": setup, "desktopChecks": desktop_checks,
            }
    return result


def outputs(source):
    template = (DOCS / "source.tmpl").read_text().strip()
    choices = profiles(source)
    yield ROOT / "website/prompts.js", (
        "// Generated by scripts/sync-install-prompt.py.\n"
        f"const angiSourceRef = {json.dumps(source)};\n"
        f"const angiSourceTemplate = {json.dumps(template)};\n"
        f"window.ANGI_INSTALL_PROFILES = {json.dumps(choices, indent=2)};\n"
        "window.ANGI_PROMPT = (target, method) => {\n"
        "  const profiles = window.ANGI_INSTALL_PROFILES;\n"
        "  const id = typeof target === 'string' ? target : target?.id;\n"
        "  if (!Object.hasOwn(profiles, id)) return 'Stop. This target has no install profile.';\n"
        "  const profile = profiles[id];\n"
        "  const mode = method ?? Object.keys(profile.modes)[0];\n"
        "  if (!Object.hasOwn(profile.modes, mode)) return 'Stop. This deployment method is unavailable.';\n"
        "  const details = profile.modes[mode];\n"
        "  const values = {...profile, ...details, sourceRef: angiSourceRef, medium: details.label};\n"
        "  return angiSourceTemplate.replace(/{{(\\w+)}}/g, (_, key) => values[key]);\n"
        "};\n"
    )
    pi_template = (DOCS / "2-connect-pi.tmpl").read_text()
    for board in published_boards(source):
        if not board["id"].startswith("rpi"):
            continue
        zero = board["id"] == "rpizero2w"
        fields = {"TARGET": board["marketing"]["short"], "LINK": ZERO_LINK if zero else PI_LINKS,
                  "SETUP": PEBBLE_SETUP,
                  "WIFI_NOTE": "\nUse a 2.4 GHz network on the Zero 2 W." if zero else ""}
        yield ROOT / board["install"]["first_boot"], re.sub(
            r"{{(\w+)}}", lambda match: fields[match[1]], pi_template)
    readme = ROOT / "README.md"
    generic = """Help me install ANGI from source. Ask me whether the target is a Pi Zero 2 W,
Pi 4, Pi 5, ARM64 UEFI, AMD x86-64 UEFI, or Intel x86-64 UEFI.
Use the matching profile and deployment method in website/prompts.js. Follow the
generated prompt, including its pinned source commit, exact-device erase
approval, SHA-256 verification, and trusted first boot."""
    yield readme, re.sub(r"(<!-- prompt1:start -->\n```text\n).*?(\n```\n<!-- prompt1:end -->)",
                         lambda m: m[1] + generic + m[2], readme.read_text(), flags=re.S)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-commit", help="exact published installer commit for the website")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    pinned = args.source_commit or (PIN.read_text().strip() if PIN.is_file() else "")
    if not re.fullmatch(r"[0-9a-f]{40}", pinned):
        parser.error("The source pin must be a full Git commit ID")
    try:
        generated = list(outputs(pinned))
    except subprocess.CalledProcessError:
        parser.exit(1, f"Cannot read the board configurations at {pinned}. Fetch the pinned source commit and retry.\n")
    if args.source_commit and not args.check:
        PIN.write_text(pinned + "\n")
    for path, content in generated:
        if args.check:
            if path.read_text() != content:
                parser.exit(1, f"Regenerate {path.relative_to(ROOT)}\n")
        else:
            path.write_text(content)


if __name__ == "__main__":
    main()
