#!/usr/bin/env python3
"""Microphone transcription contract, without contacting OpenAI or downloading models."""
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
import urllib.error
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/agent"))
import voice
import voice_local
import voice_models

PI5 = {"device": "Raspberry Pi 5 Model B Rev 1.1", "memory_gb": 15.6, "cores": 4}
PI_ZERO = {"device": "Raspberry Pi Zero 2 W", "memory_gb": 0.4, "cores": 4}


class Reply(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False


def refuse(code):
    return urllib.error.HTTPError(voice.ENDPOINT, code, "no", {}, io.BytesIO())


class Base(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        root = self.directory.name
        patcher = patch.dict(os.environ, {"XDG_DATA_HOME": root + "/data", "XDG_CONFIG_HOME": root + "/config"})
        patcher.start()
        self.addCleanup(patcher.stop)
        self.addCleanup(self.directory.cleanup)


class EngineTests(Base):
    def test_the_first_transcription_asks_which_engine_to_use(self):
        self.assertEqual(voice.run(["transcribe"], io.BytesIO(b"RIFF"))["code"], "choose-engine")

    def test_a_chatgpt_sign_in_is_not_a_credential(self):
        store = Path(self.directory.name) / "data/opencode/auth.json"
        store.parent.mkdir(parents=True)
        store.write_text(json.dumps({"openai": {"type": "oauth", "access": "token"}}))
        self.assertEqual(voice.api_key(), "")
        self.assertEqual(voice.run(["transcribe"], io.BytesIO(b"RIFF"))["code"], "choose-engine")

    def test_a_key_is_stored_privately_and_implies_the_cloud_engine(self):
        key = "sk-" + "a" * 30
        self.assertEqual(voice.run(["key"], io.StringIO(key + "\n")), {"saved": True})
        self.assertEqual(oct(voice.settings_path().stat().st_mode & 0o777), "0o600")
        seen = []
        def send(request, timeout):
            seen.append(request.get_header("Authorization"))
            self.assertIn(b'name="model"', request.data)
            return Reply(b'{"text": " hello there "}')
        with patch.object(voice.urllib.request, "urlopen", send):
            self.assertEqual(voice.run(["transcribe"], io.BytesIO(b"RIFFdata")), {"text": "hello there"})
        self.assertEqual(seen, ["Bearer " + key])

    def test_a_rejected_key_asks_for_a_new_one_without_echoing_it(self):
        key = "sk-" + "b" * 30
        voice.save_key(key)
        def send(request, timeout):
            raise refuse(401)
        with patch.object(voice.urllib.request, "urlopen", send):
            result = voice.run(["transcribe"], io.BytesIO(b"RIFFdata"))
        self.assertEqual(result["code"], "needs-key")
        self.assertNotIn(key, json.dumps(result))

    def test_malformed_keys_are_refused_and_nothing_is_saved(self):
        self.assertEqual(voice.run(["key"], io.StringIO("hunter2\n"))["code"], "bad-key")
        self.assertFalse(voice.settings_path().exists())

    def test_choosing_the_cloud_needs_a_key_and_keeps_it_when_switching(self):
        self.assertEqual(voice.run(["choose", "openai"], None)["code"], "needs-key")
        voice.save_key("sk-" + "c" * 30)
        self.assertEqual(voice.run(["choose", "openai"], None), {"engine": "openai"})
        self.assertEqual(voice.run(["choose", "local", "small"], None)["code"], "choose-engine")  # not downloaded
        self.assertTrue(voice.settings()["key"].startswith("sk-"))

    def test_network_failure_is_reported(self):
        voice.save_key("sk-" + "d" * 30)
        def send(request, timeout):
            raise OSError("down")
        with patch.object(voice.urllib.request, "urlopen", send):
            self.assertEqual(voice.run(["transcribe"], io.BytesIO(b"RIFFdata"))["code"], "network")


class ModelTests(Base):
    def test_a_pi_5_offers_practical_models_and_recommends_small(self):
        shown = {m["id"]: m for m in voice_models.practical(PI5)}
        self.assertEqual(list(shown), ["tiny", "base", "small", "medium"])
        self.assertEqual([m for m in shown if shown[m]["recommended"]], ["small"])
        self.assertTrue(shown["medium"]["slow"])
        self.assertFalse(shown["small"]["slow"])

    def test_a_small_device_only_offers_what_fits_in_memory(self):
        self.assertEqual([m["id"] for m in voice_models.practical(PI_ZERO)], [])
        tight = {"device": "Raspberry Pi 4", "memory_gb": 1.9, "cores": 4}
        shown = voice_models.practical(tight)
        self.assertEqual([m["id"] for m in shown], ["tiny", "base"])
        self.assertEqual([m["id"] for m in shown if m["recommended"]], ["base"])

    def test_a_download_is_verified_and_a_damaged_one_is_discarded(self):
        data = b"model" * 1000
        fake = {"id": "fake", "name": "Fake", "file": "ggml-fake.bin", "size": len(data), "need_gb": 1, "seconds": 1,
                "sha256": hashlib.sha256(data).hexdigest(), "quality": ""}
        with patch.object(voice_models, "MODELS", [fake]), patch.object(voice_models.urllib.request, "urlopen", lambda *a, **k: Reply(data)):
            self.assertEqual(voice.run(["install", "fake"], None), {"done": True})
            self.assertTrue(voice_models.installed(fake))
        with patch.object(voice_models, "MODELS", [{**fake, "file": "ggml-bad.bin", "sha256": "0" * 64}]), \
                patch.object(voice_models.urllib.request, "urlopen", lambda *a, **k: Reply(data)):
            self.assertIn("damaged", voice.run(["install", "fake"], None)["error"])
        self.assertEqual([p.name for p in voice_models.directory().iterdir()], ["ggml-fake.bin"])

    def test_options_describe_the_device_and_what_is_ready(self):
        with patch.object(voice_models, "hardware", lambda: PI5):
            shown = voice.run(["options"], None)
        self.assertEqual(shown["device"]["device"], PI5["device"])
        self.assertEqual([m["id"] for m in shown["models"] if m["recommended"]], ["small"])
        self.assertFalse(shown["key_saved"])
        self.assertFalse(shown["ready"])  # nothing chosen yet: the microphone opens setup at once

    def test_the_microphone_is_ready_only_once_an_engine_can_run(self):
        voice.save_key("sk-" + "e" * 30)
        self.assertTrue(voice.options()["ready"])  # a saved key implies the cloud engine
        voice.write_settings({"engine": "local", "model": "small"})
        self.assertFalse(voice.options()["ready"])  # chosen, but the model is not downloaded


class LocalTests(Base):
    def test_the_encoder_window_never_gets_tight_enough_to_repeat_sentences(self):
        self.assertEqual(voice_local.context(bytes(44) + bytes(32000)), "750")
        self.assertEqual(voice_local.context(bytes(44) + bytes(32000 * 11)), "750")
        self.assertEqual(voice_local.context(bytes(44) + bytes(32000 * 20)), "1200")
        self.assertEqual(voice_local.context(bytes(44) + bytes(32000 * 60)), "1500")

    def test_a_transcript_that_says_everything_twice_is_collapsed(self):
        said = "And so my fellow Americans."
        self.assertEqual(voice_local.collapse_repeats(said + " " + said), said)
        self.assertEqual(voice_local.collapse_repeats(" ".join([said.lower()] * 4)), said.lower())
        self.assertEqual(voice_local.collapse_repeats("And so my fellow Americans. and so my fellow americans."), said)
        self.assertEqual(voice_local.collapse_repeats("no no"), "no no")  # too short to be sure
        self.assertEqual(voice_local.collapse_repeats("Ask not. Ask what you can do."), "Ask not. Ask what you can do.")

    def test_the_language_follows_the_account(self):
        with patch.dict(os.environ, {"LC_ALL": "", "LANG": "fr_FR.UTF-8"}):
            self.assertEqual(voice_local.language(), "fr")
        with patch.dict(os.environ, {"LC_ALL": "", "LANG": "C.UTF-8"}):
            self.assertEqual(voice_local.language(), "en")

    def test_a_downloaded_model_transcribes_through_whisper_cli(self):
        data = b"model"
        fake = {"id": "fake", "name": "Fake", "file": "ggml-fake.bin", "size": len(data), "need_gb": 1, "seconds": 1,
                "sha256": hashlib.sha256(data).hexdigest(), "quality": ""}
        program = Path(self.directory.name) / "whisper-cli"
        program.write_text("#!/bin/sh\necho ' [BLANK_AUDIO] hello   local world '\n")
        program.chmod(program.stat().st_mode | stat.S_IXUSR)
        with patch.object(voice_models, "MODELS", [fake]), patch.dict(os.environ, {"ANGI_WHISPER_CLI": str(program)}):
            voice_models.directory().mkdir(parents=True)
            voice_models.path(fake).write_bytes(data)
            self.assertEqual(voice.run(["choose", "local", "fake"], None), {"engine": "local"})
            self.assertEqual(voice.run(["transcribe"], io.BytesIO(b"RIFFdata")), {"text": "hello local world"})

    def test_a_missing_binary_is_reported_plainly(self):
        fake = {"id": "fake", "name": "Fake", "file": "ggml-fake.bin", "size": 1, "need_gb": 1, "seconds": 1, "sha256": "", "quality": ""}
        with patch.object(voice_models, "MODELS", [fake]), patch.dict(os.environ, {"ANGI_WHISPER_CLI": "/nonexistent"}):
            voice.write_settings({"engine": "local", "model": "fake"})
            self.assertIn("not installed", voice.run(["transcribe"], io.BytesIO(b"RIFFdata"))["error"])


if __name__ == "__main__":
    unittest.main()
