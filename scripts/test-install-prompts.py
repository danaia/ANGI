#!/usr/bin/env python3
"""Offline prompt/board matrix and private-image failure tests; never opens media."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
REF = '02d035cf0d5e92ce35899b16761d4b67316c9b8b'
spec = importlib.util.spec_from_file_location('provision_sd', ROOT / 'scripts/provision-sd.py')
provision = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provision)


class PromptMatrix(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        js = """
const fs = require('node:fs'), vm = require('node:vm');
const context = {window:{}}; vm.createContext(context);
vm.runInContext(fs.readFileSync('website/prompts.js','utf8'), context);
console.log(JSON.stringify(Object.fromEntries(
 ['rpi5','rpi4','rpizero2w','arm-desktop','x86-desktop','bogus','__proto__','constructor']
 .map(id => [id,context.window.ANGI_PROMPT({id})]))));
"""
        cls.prompts = json.loads(subprocess.check_output(['node', '-e', js], cwd=ROOT))

    def test_board_ref_and_output_matrix(self):
        registry = json.loads(subprocess.check_output(['git', 'show', f'{REF}:scripts/boards.json'], cwd=ROOT))
        for board in registry['boards']:
            bid = board['id']
            with self.subTest(board=bid):
                prompt = self.prompts[bid]
                if board['status'] == 'planned':
                    self.assertTrue(prompt.startswith('STOP:'))
                    continue
                self.assertIn(REF, prompt)
                self.assertIn(f'BOARD={bid}', prompt)
                self.assertIn(f'--field {bid} output', prompt)
                self.assertNotIn('{{', prompt)
                self.assertNotIn('undefined', prompt)
                subprocess.run(['git', 'cat-file', '-e', f"{REF}:buildroot/configs/{board['defconfig']}"], cwd=ROOT, check=True)
                result = subprocess.run(['python3', 'scripts/detect-board.py', '--require', bid], cwd=ROOT, capture_output=True)
                self.assertEqual(result.returncode, 0)

    def test_safety_order_and_failure_instructions(self):
        for bid in ('rpi5', 'rpi4', 'rpizero2w'):
            with self.subTest(board=bid):
                p = self.prompts[bid]
                for gate in ('unsupported host', 'wrong', 'Missing or ambiguous artifacts', 'bad checksums',
                             'identity is missing or ambiguous', 'No approval means no write',
                             'StrictHostKeyChecking=yes', 'No boot or', 'scanned key alone'):
                    self.assertIn(gate, p)
                self.assertLess(p.index('sha256sum -c'), p.index('erase DEVICE'))
                self.assertLess(p.index('erase DEVICE'), p.index('--confirm-erase'))
                self.assertLess(p.index('--confirm-erase'), p.index('STEP 4 - HAND OFF'))
                self.assertIn('in this chat', p)

    def test_board_specific_setup(self):
        self.assertIn('does not support rpi4', self.prompts['rpi4'])
        self.assertIn('independent trusted per-device', self.prompts['rpi4'])
        for bid in ('rpi5', 'rpizero2w'):
            self.assertIn(f'--board {bid} and sha256sum', self.prompts[bid])
            self.assertIn('add --personalized', self.prompts[bid])
        self.assertIn('10.79.0.2', self.prompts['rpizero2w'])
        self.assertIn('micro-USB port marked USB', self.prompts['rpizero2w'])
        self.assertIn('2.4 GHz', self.prompts['rpizero2w'])
        self.assertNotIn('10.77.0.2', self.prompts['rpizero2w'])
        self.assertIn('10.78.0.2', self.prompts['rpi5'])

    def test_unknown_and_planned_stop(self):
        for bid in ('bogus', '__proto__', 'constructor', 'arm-desktop', 'x86-desktop'):
            self.assertTrue(self.prompts[bid].startswith('STOP:'))
        for bid in ('bogus', 'arm-desktop', 'x86-desktop'):
            self.assertNotEqual(subprocess.run(['python3', 'scripts/detect-board.py', '--require', bid], cwd=ROOT, capture_output=True).returncode, 0)

    def test_generated_files_are_current(self):
        paths = [ROOT / 'README.md', ROOT / 'website/prompts.js', *sorted((ROOT / 'docs/install-prompts').glob('part2-*.txt'))]
        before = [p.read_bytes() for p in paths]
        subprocess.run(['python3', 'scripts/sync-install-prompt.py'], cwd=ROOT, check=True, capture_output=True)
        self.assertEqual(before, [p.read_bytes() for p in paths])


class PrivateImageFailures(unittest.TestCase):
    def setUp(self):
        self.directory = Path(self.enterContext(tempfile.TemporaryDirectory()))
        (self.directory / 'board.txt').write_text('rpi5\n')
        (self.directory / 'sdcard.img').write_bytes(b'fixture only')
        digest = hashlib.sha256(b'fixture only').hexdigest()
        (self.directory / 'sdcard.img.sha256').write_text(digest + '  sdcard.img\n')
        (self.directory / 'known_hosts').write_bytes(b'10.77.0.2,10.78.0.2 ssh-ed25519 fixture\n')
        # No real keys or commands: isolate the bundle's board/checksum gates.
        self.enterContext(patch.object(provision.subprocess, 'run'))

    def test_valid_checksum(self):
        provision.verify_bundle(self.directory, 'rpi5')

    def test_wrong_board(self):
        with self.assertRaises(ValueError):
            provision.verify_bundle(self.directory, 'rpizero2w')

    def test_bad_checksum(self):
        (self.directory / 'sdcard.img').write_bytes(b'corrupt')
        with self.assertRaisesRegex(ValueError, 'SHA-256'):
            provision.verify_bundle(self.directory, 'rpi5')

    def test_missing_artifact(self):
        (self.directory / 'sdcard.img').unlink()
        with self.assertRaises(FileNotFoundError):
            provision.verify_bundle(self.directory, 'rpi5')

    def test_wrong_trust_record(self):
        (self.directory / 'known_hosts').write_text('untrusted\n')
        with self.assertRaisesRegex(ValueError, 'trust entry'):
            provision.verify_bundle(self.directory, 'rpi5')


if __name__ == '__main__':
    unittest.main()
