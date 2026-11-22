#!/usr/bin/env python3
"""Apple Intelligence servers refuse requests over 16 KB: the guard that keeps OpenCode's requests under it."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/agent"))
import applefit
import chat
import providers
import session
import solo

LIMIT = applefit.LIMIT


def tool(name, description="x" * 5000, parameter="y" * 400):
    return {"type": "function", "function": {"name": name, "description": description, "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": parameter}, "command": {"type": "string", "description": parameter}}}}}


def request(history=(), system_extra="", tools=3):
    system = "You are an agent.\n" + "instructions " * 300 + system_extra
    messages = [{"role": "system", "content": system}, {"role": "user", "content": "THE TASK"}, *history]
    return {"model": "base", "max_tokens": 32000, "stream": True, "messages": messages, "tools": [tool(f"t{i}") for i in range(tools)]}


def step(number, output="ok"):
    call = {"role": "assistant", "content": "", "tool_calls": [{"id": f"c{number}", "type": "function", "function": {"name": "t0", "arguments": "{}"}}]}
    return [call, {"role": "tool", "tool_call_id": f"c{number}", "content": output}]


def size(body):
    return len(applefit.dump(body))


class FitTests(unittest.TestCase):
    def test_a_request_that_fits_is_left_exactly_alone(self):
        body = request(tools=1)
        body["tools"][0]["function"]["description"] = "short"
        self.assertLess(size(body), LIMIT)
        self.assertEqual(applefit.fit(body), body)

    def test_the_first_request_of_a_real_agent_is_brought_under_the_limit(self):
        body = request(tools=9)
        self.assertGreater(size(body), 24000)
        fitted = applefit.fit(body)
        self.assertLessEqual(size(fitted), LIMIT)
        self.assertEqual([t["function"]["name"] for t in fitted["tools"]], [f"t{i}" for i in range(9)])  # no tool is lost
        self.assertEqual(fitted["messages"][1]["content"], "THE TASK")
        self.assertGreater(size(body), LIMIT)  # the caller's request is not modified

    def test_long_tool_output_is_cut_in_the_middle_and_keeps_both_ends(self):
        log = "START " + "line of build output\n" * 3000 + "FINISHED"
        fitted = applefit.fit(request(step(1, log), tools=1))
        output = fitted["messages"][-1]["content"]
        self.assertLessEqual(size(fitted), LIMIT)
        self.assertTrue(output.startswith("START") and output.endswith("FINISHED"))
        self.assertIn("left out to fit", output)

    def test_a_property_named_description_survives_shortening(self):
        fitted = applefit.fit(request(tools=9))
        properties = fitted["tools"][0]["function"]["parameters"]["properties"]
        self.assertIn("description", properties)
        self.assertEqual(properties["description"]["type"], "string")

    def test_the_skill_list_goes_before_conversation_does(self):
        listing = "<available_skills>\n" + "<skill>a skill</skill>\n" * 700 + "</available_skills>"
        body = request([m for n in range(3) for m in step(n)], system_extra="\n" + listing, tools=1)
        body["tools"][0]["function"]["description"] = "short"
        self.assertGreater(size(body), LIMIT)
        fitted = applefit.fit(body)
        self.assertNotIn("available_skills", fitted["messages"][0]["content"])
        self.assertEqual(len(fitted["messages"]), len(body["messages"]))

    def test_the_oldest_steps_go_first_and_a_tool_call_never_loses_its_result(self):
        history = [m for n in range(40) for m in step(n, "r" * 1200)]
        fitted = applefit.fit(request(history, tools=2))
        self.assertLessEqual(size(fitted), LIMIT)
        messages = fitted["messages"]
        self.assertEqual(messages[1]["content"].split("\n")[0], "THE TASK")
        self.assertIn("left out", messages[1]["content"])
        self.assertEqual(messages[-2]["tool_calls"][0]["id"], "c39")  # the latest step is kept whole
        self.assertEqual(messages[-1]["tool_call_id"], "c39")
        for index, message in enumerate(messages):
            if message.get("tool_calls"):
                self.assertEqual(messages[index + 1]["tool_call_id"], message["tool_calls"][0]["id"])
            if message.get("role") == "tool":
                self.assertTrue(messages[index - 1].get("tool_calls") or messages[index - 1].get("role") == "tool")

    def test_requests_that_are_not_chat_are_passed_through(self):
        raw = b'{"input": "' + b"x" * 20000 + b'"}'
        self.assertEqual(applefit.trim(raw), raw)
        self.assertEqual(applefit.trim(b"not json" * 5000), b"not json" * 5000)


class AgentTests(unittest.TestCase):
    def test_apple_agents_drop_the_costly_tools_and_keep_their_other_settings(self):
        profile = {"description": "d", "mode": "primary", "prompt": "PROMPT", "permission": {"*": "allow", "task": "deny"}}
        with patch.dict(os.environ, {"XDG_CONFIG_HOME": "/cfg"}):
            lean = applefit.lean(profile)
        for name in ("webfetch", "todowrite", "glob", "grep", "skill"):
            self.assertEqual(lean["permission"][name], "deny")
        self.assertEqual((lean["permission"]["*"], lean["permission"]["task"], lean["description"]), ("allow", "deny", "d"))
        self.assertIn("/cfg/opencode/skills/NAME/SKILL.md", lean["prompt"])
        self.assertNotIn("webfetch", profile["permission"])

    def test_the_provider_is_pointed_at_the_guard_and_keeps_its_models(self):
        saved = {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "http://192.168.1.175:8080/v1", "apiKey": "k"}, "models": {"base": {}}}
        pointed = applefit.provider_config(saved)
        self.assertEqual(pointed["options"], {"baseURL": applefit.URL, "apiKey": "k"})
        self.assertEqual(saved["options"]["baseURL"], "http://192.168.1.175:8080/v1")


class OnlyAppleTests(unittest.TestCase):
    """The size limits belong to Apple Intelligence's model alone: OpenAI and LM Studio runs must be unchanged."""
    APPLE = {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "http://192.168.1.175:8080/v1"}, "models": {"base": {}}}
    LMSTUDIO = {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "http://127.0.0.1:1234/v1"}, "models": {"qwen": {}}}

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        (root / "profiles").mkdir()
        for name in ("orchestrator", "builder", "tester", "os", "single"):
            (root / f"profiles/{name}.md").write_text(name.upper())
        self.root = root

    def configs(self, service, selections=None, *, legacy=False):
        """(swarm config, standalone config) with `service` active and both local servers saved."""
        with patch.object(session, "ASSETS", self.root), patch.object(session, "active", return_value=service), \
                patch.object(providers, "apple_config", return_value=self.APPLE), patch.object(providers, "lmstudio_config", return_value=self.LMSTUDIO), \
                patch.object(solo, "active_selections", return_value=selections or {}), patch.object(session, "active_selections", return_value=selections or {}):
            return json.loads(session.role_config()), json.loads(solo.config(None, "", legacy=legacy))

    def test_apple_runs_are_limited_and_routed_through_the_guard(self):
        for config in self.configs("apple"):
            self.assertEqual(config["provider"]["apple"]["options"]["baseURL"], applefit.URL)
            for profile in config["agent"].values():
                self.assertEqual(profile["permission"]["webfetch"], "deny")

    def test_a_role_without_an_apple_model_uses_one_not_another_providers_default(self):
        # Seen live: only the orchestrator had an Apple model, so the builder and tester ran on OpenCode's
        # last-used OpenAI model, failed at startup, and the orchestrator waited on them forever.
        swarm, _ = self.configs("apple", {"orchestrator": "apple/permissive"})
        self.assertEqual({name: profile["model"] for name, profile in swarm["agent"].items()}, {"angi-orchestrator": "apple/permissive", "angi-builder": "apple/permissive", "angi-tester": "apple/permissive"})
        for config in self.configs("apple"):  # nothing chosen at all: the server's first model
            self.assertEqual({profile["model"] for profile in config["agent"].values()}, {"apple/base"})

    def test_the_drawer_shows_the_model_each_role_will_actually_run_on(self):
        # The drawer and the launcher must agree, whatever mix of roles has a saved Apple model.
        everything = ("orchestrator", "builder", "tester", "os", "single")
        for saved in ({}, {"orchestrator": "apple/permissive"}, {"builder": "apple/permissive"}, {"os": "apple/permissive"}, {"single": "apple/permissive"}, {"tester": "apple/base", "builder": "apple/permissive"}):
            with self.subTest(saved=saved):
                shown = applefit.effective(saved, self.APPLE | {"models": {"base": {}, "permissive": {}}}, everything[:3], everything)
                swarm, solo_config = self.configs("apple", saved)
                for role in everything[:3]:
                    self.assertEqual(shown[role], swarm["agent"]["angi-" + role]["model"])
                self.assertEqual(shown["single"], solo_config["agent"]["angi-single"]["model"])
                _, legacy_config = self.configs("apple", saved, legacy=True)
                self.assertEqual(shown["os"], legacy_config["agent"]["angi-os"]["model"])

    def test_no_server_models_means_nothing_is_invented(self):
        self.assertEqual(applefit.effective({}, {"models": {}}, ("orchestrator",), ("orchestrator", "os")), {})

    def test_an_explicit_apple_choice_is_kept_per_role(self):
        swarm, _ = self.configs("apple", {"orchestrator": "apple/permissive", "builder": "apple/base"})
        self.assertEqual((swarm["agent"]["angi-orchestrator"]["model"], swarm["agent"]["angi-builder"]["model"], swarm["agent"]["angi-tester"]["model"]), ("apple/permissive", "apple/base", "apple/permissive"))

    def test_openai_and_lm_studio_keep_opencodes_own_model_choice(self):
        for service in ("openai", "lmstudio"):
            for config in self.configs(service):
                for profile in config["agent"].values():
                    self.assertNotIn("model", profile)

    def test_openai_runs_have_no_apple_limits_and_no_guard(self):
        for config in self.configs("openai"):
            self.assertNotIn("provider", config)
            for profile in config["agent"].values():
                self.assertEqual(set(profile["permission"]), {"*", "task"})
                self.assertNotIn("small requests", profile["prompt"])
            self.assertNotIn(str(applefit.PORT), json.dumps(config))

    def test_lm_studio_runs_keep_their_own_server_and_full_tools(self):
        for config in self.configs("lmstudio"):
            self.assertEqual(config["provider"], {"lmstudio": self.LMSTUDIO})
            for profile in config["agent"].values():
                self.assertEqual(set(profile["permission"]), {"*", "task"})
                self.assertNotIn("small requests", profile["prompt"])
            self.assertNotIn(str(applefit.PORT), json.dumps(config))

    def test_the_guard_only_starts_for_a_configured_apple_service(self):
        for service, apple, starts in (("apple", self.APPLE, True), ("openai", self.APPLE, False), ("lmstudio", self.APPLE, False), ("apple", {}, False)):
            with self.subTest(service=service, saved=bool(apple)), patch.object(session, "active", return_value=service), \
                    patch.object(providers, "apple_config", return_value=apple), patch.object(applefit, "ensure") as ensure:
                session.start_size_guard()
                self.assertEqual(ensure.called, starts)

    def test_chat_uses_the_active_provider_and_starts_the_apple_guard_before_the_first_turn(self):
        (self.root / "profiles/chat.md").write_text("Angi's chat prompt")
        skill = self.root / "skills/angi-chat"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text("---\nname: angi-chat\n---\nAngi's persona")
        original_cwd = Path.cwd()
        self.addCleanup(os.chdir, original_cwd)
        for service, model, provider in (
            ("apple", "apple/base", {"apple": applefit.provider_config(self.APPLE)}),
            ("openai", "openai/chat", None),
            ("lmstudio", "lmstudio/qwen", {"lmstudio": self.LMSTUDIO}),
        ):
            with self.subTest(service=service), patch.dict(os.environ, {"HOME": str(self.root)}), patch.object(session, "ASSETS", self.root), \
                    patch.object(session, "active", return_value=service), patch.object(providers, "apple_config", return_value=self.APPLE), \
                    patch.object(providers, "lmstudio_config", return_value=self.LMSTUDIO), \
                    patch.object(chat, "active_selections", return_value={} if service == "apple" else {"os": model}), \
                    patch.object(applefit, "ensure") as ensure:
                def run(*_, **__):
                    self.assertEqual(ensure.called, service == "apple")
                    config = json.loads(os.environ["OPENCODE_CONFIG_CONTENT"])
                    self.assertEqual(config.get("provider"), provider)
                    profile = config["agent"][chat.AGENT]
                    self.assertEqual(profile["model"], model)
                    self.assertEqual(profile["permission"]["*"], "deny")
                    self.assertEqual(profile["permission"]["angi_files_save"], "allow")
                    self.assertEqual(config["mcp"]["angi_files"]["command"][-1], str(self.root.resolve()))
                    self.assertIn("Angi's persona", profile["prompt"])
                    return subprocess.CompletedProcess([], 0, stdout='{"type":"text","part":{"text":"Hello"}}', stderr="")
                with patch.object(chat.subprocess, "run", side_effect=run):
                    self.assertEqual(chat.turn(self.root, "Hello", new=True), {"text": "Hello"})

    def test_an_unrelated_profile_is_not_modified_by_building_apples(self):
        profile = {"prompt": "P", "permission": {"*": "allow"}}
        applefit.lean(profile)
        self.assertEqual(profile, {"prompt": "P", "permission": {"*": "allow"}})


class Upstream(BaseHTTPRequestHandler):
    """Stands in for the Apple server: refuses over 16 KB like the real one, streams a reply otherwise."""
    seen = []
    bodies = []
    window = None   # when set: refuse bodies needing more than this many tokens, at `per` bytes each
    per = 2
    cap = 16384     # bodies of at least this many bytes get the 413

    def log_message(self, *_):
        pass

    def do_GET(self):
        data = b'{"data":[{"id":"base"}]}'
        self.send_response(200); self.send_header("content-length", str(len(data))); self.end_headers(); self.wfile.write(data)

    def do_POST(self):
        raw = self.rfile.read(int(self.headers["content-length"]))
        Upstream.seen.append(len(raw))
        Upstream.bodies.append(json.loads(raw))
        if Upstream.window and len(raw) / Upstream.per > Upstream.window:
            data = ('{"error":{"code":"context_length_exceeded","message":"This model\'s maximum context length is %d tokens. However, your messages resulted in %d tokens."}}' % (Upstream.window, len(raw) // Upstream.per)).encode()
            self.send_response(400); self.send_header("content-length", str(len(data))); self.end_headers(); self.wfile.write(data)
            return
        if len(raw) >= Upstream.cap:
            data = b'{"error":true,"reason":"Payload Too Large"}'
            self.send_response(413); self.send_header("content-length", str(len(data))); self.end_headers(); self.wfile.write(data)
            return
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        for piece in (b"data: one\n\n", b"data: two\n\n", b"data: [DONE]\n\n"):
            self.wfile.write(piece); self.wfile.flush()


class ProxyTests(unittest.TestCase):
    def setUp(self):
        Upstream.seen, Upstream.bodies, Upstream.window, Upstream.cap, Upstream.per = [], [], None, 16384, 2
        stats = patch.dict(applefit.STATS, {"limit": applefit.LIMIT, "trimmed": 0, "requests": 0})
        stats.start()
        self.addCleanup(stats.stop)
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
        self.guard = ThreadingHTTPServer(("127.0.0.1", 0), applefit.Handler)
        for server in (self.upstream, self.guard):
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            self.addCleanup(server.server_close)
            self.addCleanup(server.shutdown)
        config = Path(self.directory.name) / "opencode/angi-apple.json"
        config.parent.mkdir()
        config.write_text(json.dumps({"options": {"baseURL": f"http://127.0.0.1:{self.upstream.server_port}/v1"}}))
        patcher = patch.dict(os.environ, {"XDG_CONFIG_HOME": self.directory.name})
        patcher.start()
        self.addCleanup(patcher.stop)
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def post(self, body):
        req = urllib.request.Request(f"http://127.0.0.1:{self.guard.server_port}/v1/chat/completions", applefit.dump(body), {"content-type": "application/json"})
        with self.opener.open(req, timeout=10) as response:
            return response.status, response.read()

    def test_an_oversized_request_reaches_the_server_trimmed_and_is_answered_and_streamed(self):
        body = request([m for n in range(30) for m in step(n, "r" * 1500)], tools=9)
        self.assertGreater(size(body), 40000)
        status, reply = self.post(body)
        self.assertEqual(status, 200)
        self.assertEqual(reply, b"data: one\n\ndata: two\n\ndata: [DONE]\n\n")
        self.assertLessEqual(Upstream.seen[-1], LIMIT)

    def test_a_context_overflow_is_learned_from_and_retried_within_the_window(self):
        Upstream.window = 6000  # tokens, at 2 bytes each: the default budget is too much, about 7,000 bytes is right
        body = request([m for n in range(30) for m in step(n, "r" * 1500)], tools=9)
        status, _ = self.post(body)
        self.assertEqual(status, 200)
        self.assertEqual(len(Upstream.seen), 2)  # trimmed to the default budget and refused, then trimmed tighter and accepted
        self.assertLess(Upstream.seen[-1] / 2, 6000)  # inside the window
        self.assertLess(Upstream.seen[-1], Upstream.seen[0])
        self.assertEqual(applefit.STATS["limit"], int(Upstream.seen[0] * (6000 - applefit.REPLY) / (Upstream.seen[0] / 2) * 0.9))

    def test_the_tighter_budget_is_kept_for_later_requests(self):
        Upstream.window = 6000
        body = request([m for n in range(30) for m in step(n, "r" * 1500)], tools=9)
        self.post(body)
        before = len(Upstream.seen)
        self.post(body)
        self.assertEqual(len(Upstream.seen) - before, 1)  # no second refusal

    def test_a_payload_too_large_refusal_is_retried_smaller(self):
        Upstream.cap = 9000
        body = request([m for n in range(30) for m in step(n, "r" * 1500)], tools=9)
        status, _ = self.post(body)
        self.assertEqual(status, 200)
        self.assertLess(Upstream.seen[-1], 9000)

    def test_a_refusal_that_cannot_be_fixed_is_passed_on_unchanged(self):
        Upstream.cap = 100
        with self.assertRaises(urllib.error.HTTPError) as caught:
            self.post(request(tools=1))
        self.assertEqual(caught.exception.code, 413)
        self.assertIn("Payload Too Large", caught.exception.read().decode())

    def test_the_reply_length_is_capped_so_prompt_and_reply_share_the_context(self):
        body = request(tools=1)
        body["tools"][0]["function"]["description"] = "short"
        self.post(body)
        self.assertEqual(Upstream.bodies[-1]["max_tokens"], applefit.REPLY)

    def test_other_requests_are_passed_through_to_the_saved_server(self):
        with self.opener.open(f"http://127.0.0.1:{self.guard.server_port}/v1/models", timeout=10) as response:
            self.assertEqual(json.load(response), {"data": [{"id": "base"}]})

    def test_an_unreachable_server_is_reported_not_hidden(self):
        (Path(self.directory.name) / "opencode/angi-apple.json").write_text(json.dumps({"options": {"baseURL": "http://127.0.0.1:9/v1"}}))
        with self.assertRaises(urllib.error.HTTPError) as caught:
            self.post(request(tools=1))
        self.assertEqual(caught.exception.code, 502)
        self.assertIn("could not be reached", caught.exception.read().decode())


if __name__ == "__main__":
    unittest.main()
