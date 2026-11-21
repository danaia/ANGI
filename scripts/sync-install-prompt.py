#!/usr/bin/env python3
"""Render the two install prompts for the README and website from one set of templates.

  docs/install-prompts/1-card.tmpl     the one prompt the user pastes: build ANGI, write the SD card, hand off
  docs/install-prompts/2-connect.tmpl  part 2, given to the user by the agent when part 1 is done: get the system ready
  scripts/boards.json                  per-target wording

Writes:
  docs/install-prompts/part2-BOARD.txt  one rendered part 2 per target; part 1 tells the agent to print it
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
NEW_CHAT = """Read docs/install-prompts/part2-BOARD.txt from the checkout, using my board ID.
Show the full prompt in a code block and append the checkout path, private key path,
and any prepared known_hosts path. Tell me to paste it into a new chat and stop
after handing it over."""
SAME_CHAT = """Wait for me to insert the card and power on the target. Then read and follow
docs/install-prompts/part2-{id}.txt in this checkout. Carry forward its path,
the SSH private key path and any prepared known_hosts file. Verify the running
hardware and finish setup in this chat."""
CHECK_TARGET = """Install ANGI from source on my {name}. The destination target ID is {id}.
Clone the current default branch of https://github.com/danaia/ANGI.git on the
workstation and record the commit with git rev-parse HEAD. Read
.agents/skills/angi-install-media/SKILL.md from that checkout.
Run python3 scripts/detect-board.py --require {id} from its repository root.

If the checkout reports no build for {id}, tell me and stop before starting a build
or touching any drive. If it reports an experimental build, explain what remains
unverified and ask whether I want to proceed. If the checkout supports this target,
follow its documented install procedure for {id}, passing BOARD={id} when building.
If that procedure is missing, stop and tell me. Validate the documented boot artifact,
confirm the exact removable device with me before erasing it, and verify the write
and first boot. Keep the selected target."""
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
    "Connect it to this computer by Ethernet or USB-C data cable (desktops: no cable), then power it on.",
    "Wait up to 2 minutes for the first start. A Pi shows a red light, then a blinking green one.")
LINK_GENERIC = """Ask which link I use, if I did not say:
  USB-C (any Pi): USB-C data cable. Pi 10.77.0.2, me 10.77.0.1.
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
LINK_ZERO = """The Pi Zero 2 W has no Ethernet jack. I use a USB-C data cable.
Pi 10.77.0.2, me 10.77.0.1/24. Use the host's network settings to configure the
dedicated USB adapter (MAC 02:77:00:00:00:01). Leave its gateway blank and keep
my internet route in place. On Linux with NetworkManager, reuse agx-direct if present."""
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


def generic():
    p1 = render(T1, dict(WHAT="I will tell you which computer ANGI is for.", MEDIA="SD card or USB drive", MEDIA_UP="CARD",
                         ATTACHED="my microSD card (or SD card or USB drive) is attached to THIS computer, in a card reader",
                         STEP0=ASK, HANDOFF=GENERIC_HANDOFF, BOARD="<my board>",
                         PLAN_HANDOFF="Give me the second prompt to paste into a new chat.",
                         CONTINUATION=NEW_CHAT))
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
        handoff = steps(*PI_STEPS, "Connect a USB-C DATA cable (not charge-only) from the Pi Zero 2 W's data port to this "
                        "computer. This also powers it.", PI_WAIT)
    elif bid in ("rpi4", "rpi5"):
        handoff = steps(*(t.replace("the Pi", f"the {name}") for t in PI_STEPS),
                        "Connect a USB-C data cable to this computer and power on the Pi. For direct Ethernet, connect the Ethernet cable and use the Pi's own power supply.", PI_WAIT)
    else:
        handoff = steps(*(t.replace("the Pi", f"the {name}") for t in PI_STEPS),
                        "Connect it to this computer with an Ethernet cable (then use the Pi's own power supply) or a "
                        "USB-C DATA cable (not charge-only), and power it on.", PI_WAIT)
    step0 = PICK.format(name=name, id=bid)
    p1 = render(T1, dict(WHAT=f"The target is my {name}.", MEDIA=("SD card or USB drive" if desktop else "microSD card"),
                         MEDIA_UP=media.upper(), ATTACHED=attached, STEP0=step0, HANDOFF=handoff,
                         BOARD=bid, PLAN_HANDOFF="Guide me through moving the media, booting, and finishing setup.",
                         CONTINUATION=SAME_CHAT.format(id=bid)))
    if bid in ("rpi4", "rpi5"):
        return p1, render(T2_PI, dict(STEP0=step0, TARGET=name))
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
website_prompts = {}
for b in boards:
    p, c = for_target(b)
    (OUT / f"part2-{b['id']}.txt").write_text(c + "\n")
    # The shared card template implements the Pi 4 and Pi 5 SD workflows.
    website_prompts[b["id"]] = (p if b["id"] in ("rpi4", "rpi5")
                                else CHECK_TARGET.format(name=b["name"], id=b["id"]))
statuses = {b["id"]: b["status"] for b in boards}
(ROOT / "website/prompts.js").write_text(
    "// Generated by scripts/sync-install-prompt.py. Edit the templates and regenerate.\n"
    f"window.ANGI_BUILD_STATUS = {json.dumps(statuses, indent=2)};\n"
    f"const angiInstallPrompts = {json.dumps(website_prompts, indent=2)};\n"
    "window.ANGI_PROMPT = (target) => angiInstallPrompts[target.id];\n"
)
print("Prompts synced: README.md, docs/install-prompts/part2-*.txt, and website/prompts.js.")
