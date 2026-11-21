#!/usr/bin/env python3
"""Render the shared source install prompt for the README and website from one set of templates.

  docs/install-prompts/1-card.tmpl     the one prompt the user pastes: build ANGI, write the SD card, hand off
  docs/install-prompts/2-connect.tmpl  first-boot handoff for the same chat
  scripts/boards.json                  per-target wording

Writes:
  docs/install-prompts/part2-BOARD.txt  first-boot instructions per target
  README.md                 between <!-- prompt1:start/end -->                (generic prompt)
  website/prompts.js        build prompt and status for each hardware tab
Run after editing a template or boards.json:  python3 scripts/sync-install-prompt.py
"""
import json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
T1 = (ROOT / "docs/install-prompts/1-card.tmpl").read_text().rstrip("\n")
T2 = (ROOT / "docs/install-prompts/2-connect.tmpl").read_text().rstrip("\n")
T2_PI = (ROOT / "docs/install-prompts/2-connect-pi.tmpl").read_text().rstrip("\n")
profiles = {}
boards = json.loads((ROOT / "scripts/boards.json").read_text())["boards"]

ASK = """STEP 0 - ASK WHAT ANGI IS GOING ON
If I have not already said, ask me this and wait for the answer:
  1) Raspberry Pi Zero 2 W        (rpizero2w)
  2) Raspberry Pi 4 or 400        (rpi4)
  3) Raspberry Pi 5               (rpi5)
  4) ARM desktop or laptop        (arm-desktop)
  5) Intel/AMD desktop or laptop  (x86-desktop)
  6) Not sure - help me tell
If I answer 6, run scripts/detect-board.py --list and --host and show me the "how to
tell" lines (for a Pi, ask me to read the name printed on the board). Then run
scripts/detect-board.py --require BOARD and follow what it reports. If it says a
target has no image yet, stop. If a build is experimental, explain what remains
unverified and ask if I want to proceed. Use only the selected board and the
removable drive I approve.
"""
PICK = """STEP 0 - TARGET
I am installing ANGI on: {name} (BOARD={id}). Do not ask me which one.
Run python3 scripts/detect-board.py --require {id}. If it reports no build, stop
and tell me. If it reports an experimental build, explain what remains unverified
and ask whether I want to proceed.
"""
GENERIC_CONTINUATION = """Continue in this chat after I insert the card and power on the target.
Read docs/install-prompts/part2-BOARD.txt using my selected board ID. Carry forward
the checkout, key and trust-file paths. Stop on boot, network or SSH failure;
never disable strict host-key checking or trust an unverified scanned key."""
SAME_CHAT = """Wait for me to insert the card and power on the target. Then read and follow
docs/install-prompts/part2-{id}.txt in this checkout. Carry forward its path,
the SSH private key path and any prepared known_hosts file. Verify the running
hardware and finish setup in this chat."""
PI_STEPS = ("Power off the Pi and unplug it.",
            "Take the finished card out of the card reader on this computer.",
            "Push the card into the Pi's microSD slot (gold contacts toward the board) until it seats.")
PI_WAIT = "Wait up to 2 minutes for it to start: the red light is power, a blinking green light means it is reading the card."


def steps(*items):
    return "\n".join(f"  {i}) {t}" for i, t in enumerate(items, 1))


GENERIC_HANDOFF = steps(
    "Power off the target (a desktop: shut it down).",
    "Take the finished card out of the card reader on this computer.",
    "Put it in the target: push a microSD card into the Pi's slot, gold contacts toward the board, until it seats. "
    "A desktop: plug the drive in.",
    "Connect the Pi 4 or Pi 5 by USB-C or direct Ethernet. For the Zero 2 W, use the micro-USB port marked USB.",
    "Power on the target and allow up to 2 minutes for first boot.")
LINK_GENERIC = """Ask which link I use, if I did not say:
  USB-C (Pi 4 / Pi 5): data cable. Pi 10.77.0.2, me 10.77.0.1.
  MICRO-USB (Zero 2 W): port marked USB. Pi 10.79.0.2, me 10.79.0.1.
  ETHERNET (Pi 4 / Pi 5): RJ45 cable and the Pi's own power supply. Pi 10.78.0.2, me 10.78.0.1.
  DESKTOP (ARM or Intel/AMD): no cable. Help me find it on my network or use its screen.
Use my host's network settings on the dedicated adapter. For USB, identify the
adapter by MAC 02:77:00:00:00:01. For Ethernet, use a spare wired port. For both,
use a /24 prefix and leave the gateway blank. Keep my existing internet route.
On Linux with NetworkManager, reuse agx-direct for USB or agx-ethernet for RJ45."""
LINK_USBC_ETH = """Ask me if I am using USB-C or Ethernet, if I did not say.
USB-C: data cable to the Pi's USB-C port. Pi 10.77.0.2, me 10.77.0.1/24.
Identify the USB Ethernet adapter by MAC 02:77:00:00:00:01.
ETHERNET: RJ45 cable and the Pi's own power supply. Pi 10.78.0.2, me 10.78.0.1. Create or
reuse a dedicated wired connection with a /24 prefix. If that port carries my
internet, stop and ask. Configure the dedicated adapter in the host's network
settings, leave its gateway blank, and keep my internet route in place. On Linux
with NetworkManager, reuse agx-direct or agx-ethernet if present."""
LINK_ZERO = """Connect a data cable to the Zero 2 W micro-USB port marked USB.
Use 10.79.0.1/24 on the workstation and reach the Pi at 10.79.0.2. Identify the
adapter by MAC 02:79:00:00:00:01. Configure only that link, with no gateway or
default route. On Linux with NetworkManager, reuse angi-zero-usb if it exists.
Use the private image's per-card known_hosts file. See docs/pi-zero-2w.md."""
LINK_DESKTOP = """There is no cable profile for this target. Ask me how it is reachable (it is on my network,
or I have its screen and keyboard) and help me connect that way."""
FIND_PI = """Wait up to 2 minutes for it to boot. For USB, look for a new Ethernet adapter. For
direct Ethernet, check that the dedicated wired port has a link. Confirm the route and
ping it. If it does not appear, go through: data cable (not charge-only)? powered, with the
red LED on and the green LED blinking? another USB port or the Pi's own supply? card fully
seated? Fix what you can and tell me what to check."""
FIND_DESKTOP = """Wait for it to start from the drive. Help me find its address (ask me, or check my router
or the screen) and confirm I can reach it. If it does not start, help me find the boot menu
key and choose the drive."""
WIFI_ZERO = "\nThe Pi Zero 2 W only supports 2.4 GHz Wi-Fi, so help me use a 2.4 GHz network."


def render(t, vals):
    for k, v in vals.items():
        t = t.replace("{{" + k + "}}", v)
    left = re.findall(r"{{\w+}}", t)
    if left:
        sys.exit(f"unfilled placeholders: {left}")
    return t


PRIVATE_SD = """For this board, ensure dropbearkey is available, then run
python3 scripts/provision-sd.py --images IMAGE_DIR --output-dir PRIVATE_PARENT
--public-key PUBLIC_KEY, with a private directory outside the checkout.
Use the returned private directory, run provision-sd.py --verify PRIVATE_DIR
--board BOARD_ID and sha256sum -c sdcard.img.sha256 there. Use only its sdcard.img
and prepared known_hosts for flashing and first SSH. Stop if either check fails."""
PI4_SD = """Use the validated build sdcard.img directly. Do not run provision-sd.py:
it does not support rpi4. The Pi creates its host key on first boot. Before SSH,
verify that key through the Pi's local console or an independent trusted per-device
record. If neither is available, STOP and explain the missing trust step.
Never use ssh-keyscan output alone as proof of identity."""
GENERIC_PROVISION = "For rpi4: " + PI4_SD + "\nFor rpi5 or rpizero2w: " + PRIVATE_SD.replace("BOARD_ID", "the selected board ID")


def generic():
    p1 = render(T1, dict(WHAT="I will tell you which computer ANGI is for.", MEDIA="SD card or USB drive", MEDIA_UP="CARD",
                         ATTACHED="my microSD card (or SD card or USB drive) is attached to THIS computer, in a card reader",
                         STEP0=ASK, HANDOFF=GENERIC_HANDOFF, BOARD="<my board>",
                         PLAN_HANDOFF="Guide boot and setup in this same chat.",
                         PROVISION=GENERIC_PROVISION, FLASH="Use --personalized only for the private Pi 5 or Zero 2 W image.",
                         CONTINUATION=GENERIC_CONTINUATION))
    p2 = render(T2, dict(WHAT="My ANGI card from part 1 is in the target and it is connected to this computer.",
                         STEP0="STEP 0 - WHICH ONE\nAsk me which target this is (Pi Zero 2 W, Pi 4 or 400, Pi 5, ARM computer, Intel/AMD computer) if\nI did not say, and run scripts/detect-board.py --require BOARD. Follow what it reports.\n",
                         LINK=LINK_GENERIC, FIND=FIND_PI, EXTRA="", WIFI_NOTE=""))
    return p1, p2


def for_target(b):
    name, bid = b["marketing"]["short"], b["id"]
    desktop = bid.endswith("desktop")
    media = "drive" if desktop else "card"
    attached = ("my SD card or USB drive is attached to THIS computer" if desktop
                else "a card reader with my microSD card is attached to THIS computer")
    if desktop:
        handoff = steps("Unplug the finished drive from this computer.",
                        "Plug it into the computer ANGI is going on, and start that computer from the drive (boot menu).",
                        "Wait for it to start.")
    elif bid == "rpizero2w":
        handoff = steps(*PI_STEPS, "Connect the micro-USB port marked USB to this computer using a data cable.",
                        "Use the separate power port if the workstation cannot reliably supply power. Allow up to 2 minutes for first boot.")
    elif bid in ("rpi4", "rpi5"):
        handoff = steps(*(t.replace("the Pi", f"the {name}") for t in PI_STEPS),
                        "Connect a USB-C data cable with adequate power, or use direct Ethernet and the Pi's own suitable power supply. Do not assume a workstation USB port supplies enough power.", PI_WAIT)
    else:
        handoff = steps(*(t.replace("the Pi", f"the {name}") for t in PI_STEPS),
                        "Connect it to this computer with an Ethernet cable (then use the Pi's own power supply) or a "
                        "USB-C DATA cable (not charge-only), and power it on.", PI_WAIT)
    step0 = PICK.format(name=name, id=bid)
    vals = dict(WHAT=f"The target is my {name}.", MEDIA=("SD card or USB drive" if desktop else "microSD card"),
                         MEDIA_UP=media.upper(), ATTACHED=attached, STEP0=step0, HANDOFF=handoff,
                         BOARD=bid, PLAN_HANDOFF="Guide me through moving the media, booting, and finishing setup.",
                         CONTINUATION=SAME_CHAT.format(id=bid),
                         PROVISION=PI4_SD if bid == "rpi4" else PRIVATE_SD.replace("BOARD_ID", bid),
                         FLASH="Use the checked build sdcard.img without --personalized." if bid == "rpi4" else "Use the verified private sdcard.img and add --personalized.")
    profiles[bid] = vals
    p1 = render(T1, vals)
    if bid in ("rpi4", "rpi5", "rpizero2w"):
        return p1, render(T2_PI, dict(STEP0=step0, TARGET=name,
                                     LINK=LINK_ZERO if bid == "rpizero2w" else LINK_USBC_ETH,
                                     WIFI_NOTE=WIFI_ZERO if bid == "rpizero2w" else ""))
    link = LINK_DESKTOP if desktop else LINK_ZERO if bid == "rpizero2w" else LINK_USBC_ETH
    p2 = render(T2, dict(WHAT=f"The target is my {name}. The card from part 1 is in it and it is connected to this computer.",
                         STEP0=step0, LINK=link, FIND=FIND_DESKTOP if desktop else FIND_PI,
                         EXTRA="",
                         WIFI_NOTE=WIFI_ZERO if bid == "rpizero2w" else ""))
    return p1, p2


def sub_block(path, pattern, repl):
    text = path.read_text()
    text, n = re.subn(pattern, repl, text, flags=re.S)
    if n != 1:
        sys.exit(f"{path.name}: expected one match for {pattern[:40]!r}, found {n}")
    path.write_text(text)


g1, g2 = generic()
readme = ROOT / "README.md"
for i, g in ((1, g1),):
    sub_block(readme, rf"(<!-- prompt{i}:start -->\n```text\n).*?(\n```\n<!-- prompt{i}:end -->)",
              lambda m, g=g: m.group(1) + g + m.group(2))
OUT = ROOT / "docs/install-prompts"

for b in boards:
    p, c = for_target(b)
    (OUT / f"part2-{b['id']}.txt").write_text(c + "\n")
statuses = {b["id"]: b["status"] for b in boards}
(ROOT / "website/prompts.js").write_text(
    "// Generated by scripts/sync-install-prompt.py. Edit the templates and regenerate.\n"
    f"window.ANGI_BUILD_STATUS = {json.dumps(statuses, indent=2)};\n"
    f"const angiSourceTemplate = {json.dumps(T1)};\n"
    f"const angiSourceProfiles = {json.dumps({k: v for k, v in profiles.items() if k in ('rpi4', 'rpi5', 'rpizero2w')}, indent=2)};\n"
    f"const angiFirstBoot = {json.dumps({b['id']: for_target(b)[1] for b in boards if b['id'] in ('rpi4', 'rpi5', 'rpizero2w')}, indent=2)};\n"
    "window.ANGI_PROMPT = (target) => {\n"
    "  const id = target && target.id;\n"
    "  if (!Object.hasOwn(angiSourceProfiles, id)) return 'STOP: no supported source install profile for this target. Do not build or write media.';\n"
    "  const profile = angiSourceProfiles[id];\n"
    "  return angiSourceTemplate.replace(/{{(\\w+)}}/g, (_, key) => profile[key]) + '\\n\\n' + angiFirstBoot[id];\n"
    "};\n"
)
print("Prompts synced: README.md, docs/install-prompts/part2-*.txt, and website/prompts.js.")
