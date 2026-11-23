#!/usr/bin/env python3
"""Exercise the source-install gates and every generated profile without disks."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from detect_board_registry import BY_ID, ROOT
from iso_media import sha256_file


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


install = load("install-source")
provision = load("provision-sd")


class WebsitePrompts(unittest.TestCase):
    def test_all_choices_and_deployments(self):
        javascript = """
global.window = {};
require('./website/prompts.js');
const profiles = window.ANGI_INSTALL_PROFILES;
const outputs = Object.fromEntries(Object.entries(profiles).map(([id, profile]) =>
  [id, Object.fromEntries(Object.keys(profile.modes).map(mode =>
    [mode, window.ANGI_PROMPT(id, mode)]))]));
console.log(JSON.stringify(outputs));
"""
        output = subprocess.check_output(["node", "-e", javascript], cwd=ROOT, text=True)
        prompts = json.loads(output)
        self.assertEqual(set(prompts), {"rpizero2w", "rpi4", "rpi5", "arm-desktop", "amd", "intel"})
        pin = (ROOT / "website/source-commit.txt").read_text().strip()
        for board, config in BY_ID.items():
            for choice in config["install"]["choices"]:
                selected = prompts[choice["id"]]
                self.assertEqual(set(selected), set(config["install"]["media"]))
                for mode, prompt in selected.items():
                    with self.subTest(target=choice["id"], deployment=mode):
                        self.assertNotRegex(prompt, r"{{|undefined")
                        self.assertIn(f"Fetch\n{pin}", prompt)
                        for command in ("configure", "dependencies", "build", "check-image"):
                            self.assertIn(f"make {command} BOARD={board}\n", prompt)
                        for text in (f"--require {board}", config["install"]["first_boot"],
                                     config["image"] + ".sha256", "StrictHostKeyChecking=yes",
                                     "HostKeyAlias=" + config["install"]["ssh_host_alias"]):
                            self.assertIn(text, prompt)
                        self.assertTrue((ROOT / config["install"]["first_boot"]).is_file())
                        if mode == "ethernet":
                            self.assertIn("--interface IFACE", prompt)
                        else:
                            self.assertIn("If more than\none device could be mine, stop", prompt)
                            self.assertLess(prompt.index("erase PATH"), prompt.index("--confirm-erase"))
                            self.assertLess(prompt.index("ejection"), prompt.index("move it"))
                        if board == "rpi5":
                            for text in ("Choose a session at sign-in", "Choose Native",
                                         "Choose Full Desktop at", "local HDMI", "seatd", "Weston"):
                                self.assertIn(text, prompt)
                        elif board in {"rpi4", "rpizero2w"}:
                            self.assertIn("Choose Native", prompt)
                            self.assertNotIn("Full Desktop", prompt)
                        else:
                            self.assertIn("text login", prompt)
                            self.assertIn("agx tui", prompt)
                            self.assertNotIn("VNC", prompt)
                        if board.startswith("rpi"):
                            for text in ("Pebble first-boot wizard", "Pair Bluetooth devices",
                                         "127.0.0.1:5900", "passwords directly in the wizard"):
                                self.assertIn(text, prompt)
        self.assertIn("verify the software VNC backend", prompts["rpizero2w"]["sd"])
        self.assertIn("micro-USB port marked USB", prompts["rpizero2w"]["sd"])
        self.assertIn("AuthenticAMD", prompts["amd"]["usb"])
        self.assertIn("GenuineIntel", prompts["intel"]["usb"])


class SourceGates(unittest.TestCase):
    def test_unsupported_checkout_lacks_defconfig(self):
        with tempfile.TemporaryDirectory() as temp:
            scripts = Path(temp) / "scripts"
            scripts.mkdir()
            for name in ("detect-board.py", "boards.json", "detect_board_registry.py"):
                shutil.copy2(ROOT / "scripts" / name, scripts)
            result = subprocess.run([sys.executable, "-I", scripts / "detect-board.py",
                                     "--require", "rpi5"], text=True, capture_output=True)
            self.assertEqual(result.returncode, 3)
            self.assertIn("lacks the rpi5 defconfig", result.stderr)

    def test_no_implicit_device_choice(self):
        result = subprocess.run([sys.executable, ROOT / "scripts/install-source.py", "prepare",
                                 "--board", "rpi5", "--output-dir", "/tmp"],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("--device", result.stderr)
        self.assertNotIn("Source commit", result.stdout)

    def test_ambiguous_device_list_stops_before_build(self):
        result = subprocess.run([sys.executable, ROOT / "scripts/install-source.py", "prepare",
                                 "--board", "rpi5", "--output-dir", "/tmp",
                                 "--device", "/dev/reader1", "--device", "/dev/reader2"],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("Select exactly one device", result.stderr)
        self.assertNotIn("Source commit", result.stdout)

    def test_failed_image_check_stops_before_provisioning(self):
        steps = []

        def run(*args, **kwargs):
            if args[:2] == ("git", "rev-parse"):
                return subprocess.CompletedProcess(args, 0, stdout="0" * 40)
            if args[:2] == ("git", "status"):
                return subprocess.CompletedProcess(args, 0, stdout="")
            if args[1] == "scripts/ssh_provisioning.py":
                return subprocess.CompletedProcess(args, 0, stdout="/tmp/identity.pub")
            steps.append(args)
            if args[:2] == ("make", "check-image"):
                raise subprocess.CalledProcessError(1, args)
            return subprocess.CompletedProcess(args, 0)

        with patch.object(install, "run", run), patch.object(install, "preflight"):
            with self.assertRaises(subprocess.CalledProcessError):
                install.prepare("rpi4", "/dev/example", None, Path("/tmp"))
        self.assertEqual(steps, [("make", step, "BOARD=rpi4") for step in
                                 ("configure", "dependencies", "build", "check-image")])

    def test_prepared_bundle_detects_changed_image_and_changed_pin(self):
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            board = "x86-desktop"
            (bundle / "board.txt").write_text(board + "\n")
            image = bundle / "disk.img"
            trust = bundle / "known_hosts"
            image.write_bytes(b"file fixture")
            login = bundle / "fixture"
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", login], check=True)
            public = login.with_suffix(".pub").read_bytes()
            trust.write_bytes(provision.host_names(board) + b" " + public)
            digest = sha256_file(image)
            (bundle / "disk.img.sha256").write_text(digest + "  disk.img\n")
            (bundle / "install.json").write_text(json.dumps({
                "board": board, "image_sha256": digest, "known_hosts_sha256": sha256_file(trust),
            }))
            provision.verify_bundle(bundle, board)
            image.write_bytes(b"corrupt")
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                provision.verify_bundle(bundle, board)
            image.write_bytes(b"file fixture")
            trust.write_bytes(provision.host_names(board) + b" " + public.strip() + b" changed\n")
            with self.assertRaisesRegex(ValueError, "trust record"):
                provision.verify_bundle(bundle, board)


if __name__ == "__main__":
    unittest.main()
