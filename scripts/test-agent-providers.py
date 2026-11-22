#!/usr/bin/env python3
"""Provider setup contract without contacting or authenticating real accounts."""
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer, ThreadingHTTPServer
import unittest.mock
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/agent"))
import providers
import models
import session


class ProviderTests(unittest.TestCase):
    def test_apple_discovery_persists_configuration_used_by_roles(self):
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                self.server.requested = self.path
                self.send_response(200)
                self.end_headers()
                models = b'{"data":[{"id":"base"}]}' if self.server.model_available else b'{"data":[]}'
                self.wfile.write(models)
            def log_message(self, *_):
                pass
        server = HTTPServer(("127.0.0.1", 0), Handler)
        server.model_available = True
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_CONFIG_HOME": temporary + "/config", "XDG_STATE_HOME": temporary + "/state"}):
                providers.connect_apple(f"http://127.0.0.1:{server.server_port}")
                self.assertEqual(server.requested, "/v1/models")
                self.assertEqual(models.catalog("apple"), ["apple/base"])
                with self.assertRaises(ValueError):  # not the service in use yet
                    models.save("tester", "apple/base")
                models.activate("apple")
                models.save("tester", "apple/base")
                profiles = Path(temporary) / "profiles"
                profiles.mkdir()
                for role in models.ROLES:
                    (profiles / f"{role}.md").write_text(role)
                with patch.object(session, "ASSETS", Path(temporary)):
                    config = json.loads(session.role_config())
                self.assertEqual(config["agent"]["angi-tester"]["model"], "apple/base")
                self.assertEqual(config["provider"]["apple"]["models"], {"base": {"name": "base"}})
                service = providers.Providers()
                self.assertTrue(service.status("apple")["connected"])
                server.model_available = False
                self.assertFalse(service.status("apple")["connected"])
                apple_card = providers.overview(service.apple_health)["apple"]
                self.assertFalse(apple_card["connected"])
                self.assertTrue(apple_card["active"])
                self.assertEqual(apple_card["models"]["tester"], "apple/base")
                server.model_available = True
                self.assertTrue(service.status("apple")["connected"])
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_openai_status_requires_credentials_and_filters_catalog(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_DATA_HOME": temporary, "XDG_STATE_HOME": temporary}):
            service = providers.Providers()
            self.assertFalse(service.status("openai")["connected"])
            auth = Path(temporary) / "opencode/auth.json"
            auth.parent.mkdir()
            auth.write_text('{"openai":{"type":"oauth","access":"private"}}')
            catalog = {"all": [{"id":"openai", "models": {"coding": {"capabilities":{"toolcall":True}}, "image": {"capabilities":{"toolcall":False}}}}]}
            with patch.object(service, "api", return_value=catalog):
                status = service.status("openai")
            self.assertEqual(status["models"], ["openai/coding"])
            self.assertTrue(status["connected"])
            self.assertNotIn("private", json.dumps(status))

    def test_apple_url_rejects_embedded_credentials(self):
        with self.assertRaises(ValueError):
            providers.connect_apple("http://user:password@localhost:1234")

    def test_apple_remembers_submitted_address_when_server_is_offline(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_CONFIG_HOME": temporary, "XDG_STATE_HOME": temporary}):
            config = {"options": {"baseURL": "http://previous-device:1234/v1"}, "models": {"base": {}}}
            models.atomic_write(Path(temporary) / "opencode/angi-apple.json", config)
            address = "http://new-device:1234"
            with patch.object(providers.urllib.request, "urlopen", side_effect=OSError("offline")) as request:
                self.assertEqual(providers.Providers().status("apple")["address"], config["options"]["baseURL"])
                with self.assertRaises(OSError):
                    providers.connect_apple(f" {address} ")
                status = providers.Providers().status("apple")
                request.assert_called_with(config["options"]["baseURL"] + "/models", timeout=2)
            self.assertEqual(status["address"], address)
            self.assertFalse(status["connected"])
            self.assertEqual(providers.apple_config(), config)

    def test_oauth_is_started_using_runtime_method_and_callback(self):
        service = providers.Providers()
        finished = threading.Event()
        calls = []
        def api(path, data=None, timeout=30):
            calls.append((path, data))
            if path == "/provider/auth":
                return {"openai": [{"type":"api","label":"Key"}, {"type":"oauth","label":"Browser sign-in"}]}
            if path.endswith("authorize"):
                return {"url":"https://example.com/oauth", "method":"auto", "instructions":"Continue"}
            finished.set()
            return True
        with patch.object(service, "api", side_effect=api):
            self.assertEqual(service.connect()["url"], "https://example.com/oauth")
            self.assertTrue(finished.wait(2))
        self.assertIn(("/provider/openai/oauth/callback", {"method":1}), calls)

    def test_requests_wait_for_a_server_that_swallows_early_connections(self):
        # OpenCode opens its port before it serves; a request sent then is never answered.
        release = threading.Event()
        seen = []
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append(self.path)
                if len(seen) == 1:
                    release.wait(10)
                    return
                self.send_response(200)
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"{}")
            def log_message(self, *_):
                pass
        class Server(ThreadingHTTPServer):
            daemon_threads = True
        server = Server(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            service = providers.Providers()
            service.url = f"http://127.0.0.1:{server.server_port}"
            service.process = unittest.mock.Mock(poll=lambda: None)
            service.wait_until_serving(deadline=20)
            self.assertEqual(seen[:2], ["/global/health", "/global/health"])
            self.assertEqual(service.api("/provider/auth"), {})
            self.assertEqual(seen[-1], "/provider/auth")
        finally:
            release.set()
            server.shutdown()
            thread.join()
            server.server_close()

    def test_exited_service_fails_instead_of_waiting(self):
        service = providers.Providers()
        service.url = "http://127.0.0.1:9"
        service.process = unittest.mock.Mock(poll=lambda: 1)
        with self.assertRaises(RuntimeError):
            service.wait_until_serving(deadline=20)

    def test_failed_start_stops_the_service_so_next_request_restarts(self):
        service = providers.Providers()
        process = unittest.mock.Mock(poll=lambda: None)
        with patch.object(providers.subprocess, "Popen", return_value=process), patch.object(service, "wait_until_serving", side_effect=RuntimeError("not ready")):
            with self.assertRaises(RuntimeError):
                service.api("/provider/auth")
        process.terminate.assert_called()
        self.assertIsNone(service.process)

    def test_using_a_service_requires_a_connection_and_is_exclusive(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_DATA_HOME": temporary + "/data", "XDG_STATE_HOME": temporary + "/state", "XDG_CONFIG_HOME": temporary + "/config"}):
            service = providers.Providers()
            self.assertEqual(providers.overview()["openai"], {"connected": False, "active": False, "models": {}})
            for name in ("openai", "apple", "lmstudio"):
                with self.assertRaises(ValueError):
                    service.use(name)
            auth = Path(temporary) / "data/opencode/auth.json"
            auth.parent.mkdir(parents=True)
            auth.write_text('{"openai":{"type":"oauth","access":"private"}}')
            apple = {"npm": "x", "options": {"baseURL": "http://device:1234/v1"}, "models": {"base": {}}}
            with patch.object(service, "api", return_value={"all": []}), patch.object(providers, "apple_config", return_value=apple), patch.object(providers, "apple_model_ids", return_value=["base"]):
                first = service.use("openai")
                self.assertTrue(first["active"])
                self.assertIn("default model", first["status"])  # no models chosen yet
                chosen = {"builder": "openai/gpt-x"}
                models.atomic_write(models.settings_path(), chosen)
                # Apple has no saved choice, so its card shows the model every role would fall back to, not nothing.
                everything = {role: "apple/base" for role in ("orchestrator", "builder", "tester", "os", "single")}
                self.assertTrue(service.status("apple")["connected"])
                self.assertEqual(providers.overview(service.apple_health), {"apple": {"configured": True, "connected": True, "active": False, "models": everything}, "openai": {"connected": True, "active": True, "models": chosen}, "lmstudio": {"connected": False, "active": False, "models": {}}})
                self.assertTrue(service.use("apple")["active"])
                self.assertFalse(service.status("openai")["active"])
                overview = providers.overview(service.apple_health)
            self.assertEqual([name for name, card in overview.items() if card["active"]], ["apple"])
            self.assertNotIn("private", json.dumps(overview))

    def test_bridge_reports_every_card_with_each_response(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_DATA_HOME": temporary, "XDG_STATE_HOME": temporary, "XDG_CONFIG_HOME": temporary}):
            lines = ['{"action":"overview"}\n', '{"action":"activate","provider":"claude"}\n']
            with patch("sys.stdin", lines), patch("builtins.print") as printed:
                providers.bridge()
            first, second = (json.loads(call.args[0]) for call in printed.call_args_list)
            self.assertEqual(sorted(first["overview"]), ["apple", "lmstudio", "openai"])
            self.assertIn("Unknown provider", second["error"])
            self.assertIn("overview", second)

    def test_lmstudio_address_forms_and_rejections(self):
        self.assertEqual(providers.parse_lmstudio("angi-pc"), ("angi-pc", 1234))
        self.assertEqual(providers.parse_lmstudio(" dana-pc.tail1234.ts.net:4321 "), ("dana-pc.tail1234.ts.net", 4321))
        self.assertEqual(providers.parse_lmstudio("http://100.64.0.7:1234/v1/"), ("100.64.0.7", 1234))
        for bad in ("", "https://pc", "http://user:pw@pc", "pc:notaport", "http://pc/other", "http://pc?x=1", "-oProxyCommand=x", "pc;rm", "a b"):
            with self.assertRaises(ValueError, msg=bad):
                providers.parse_lmstudio(bad)

    def test_lmstudio_connect_tests_through_the_relay_and_keeps_the_old_target_on_failure(self):
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200)
                self.send_header("Content-Length", str(len(self.server.body)))
                self.end_headers()
                self.wfile.write(self.server.body)
            def log_message(self, *_):
                pass
        server = HTTPServer(("127.0.0.1", 0), Handler)
        server.body = b'{"data":[{"id":"qwen3-coder"},{"id":"text-embedding-nomic"}]}'
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_CONFIG_HOME": temporary + "/config", "XDG_STATE_HOME": temporary + "/state"}), \
                    patch.object(providers.tailnet, "TARGET", Path(temporary) / "tailscale/lmstudio-target"), \
                    patch.object(providers.tailnet, "RELAY", f"http://127.0.0.1:{server.server_port}/v1"):
                with patch.object(providers.tailnet, "state", return_value={"ready": False, "login": True, "text": ""}):
                    with self.assertRaises(ValueError):  # Tailscale not signed in
                        providers.connect_lmstudio("angi-pc")
                with patch.object(providers.tailnet, "state", return_value={"ready": True, "login": False, "text": ""}):
                    config = providers.connect_lmstudio("angi-pc:4321")
                    self.assertEqual(providers.tailnet.target(), ("angi-pc", 4321))
                    self.assertEqual(list(config["models"]), ["qwen3-coder"])  # embedding models are not chat models
                    self.assertEqual(config["options"]["baseURL"], providers.tailnet.RELAY)
                    self.assertEqual(providers.lmstudio_config(), config)
                    server.body = b'{"data":[]}'
                    with self.assertRaises(ValueError):
                        providers.connect_lmstudio("other-pc")
                    self.assertEqual(providers.tailnet.target(), ("angi-pc", 4321))  # restored
                    self.assertEqual(models.catalog("lmstudio"), ["lmstudio/qwen3-coder"])
                    self.assertTrue(providers.overview()["lmstudio"]["connected"])
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_tailscale_sign_in_returns_only_the_tailscale_login_page(self):
        def result(out, err=""):
            return unittest.mock.Mock(stdout=out, stderr=err)
        with patch.object(providers.tailnet, "run", return_value=result("", "\nTo authenticate, visit:\n\n\thttps://login.tailscale.com/a/abc123\n")):
            self.assertEqual(providers.tailnet.login(), "https://login.tailscale.com/a/abc123")
        with patch.object(providers.tailnet, "run", return_value=result("Success.")):
            self.assertIsNone(providers.tailnet.login())  # already signed in: nothing to approve
        with patch.object(providers.tailnet, "run", side_effect=[result("visit https://evil.example/a/x"), result('{"AuthURL":"https://evil.example/a/x"}')]):
            self.assertIsNone(providers.tailnet.login())

    def test_panel_says_it_is_waiting_only_while_a_sign_in_is_pending(self):
        service = providers.Providers()
        waiting = {"ready": False, "login": True, "text": "Sign in"}
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_CONFIG_HOME": temporary, "XDG_STATE_HOME": temporary}):
            with patch.object(providers.tailnet, "state", return_value=dict(waiting)):
                self.assertEqual(service.status("lmstudio")["tailscale"]["text"], "Sign in")
                service.tailscale_waiting = True
                self.assertIn("Waiting", service.status("lmstudio")["tailscale"]["text"])
            with patch.object(providers.tailnet, "state", return_value={"ready": True, "login": False, "text": "Connected"}):
                self.assertEqual(service.status("lmstudio")["tailscale"]["text"], "Connected")
            self.assertFalse(service.tailscale_waiting)

    def test_sign_in_repeats_the_safe_preferences_and_state_restores_shields(self):
        calls = []
        def run(*arguments, **_):
            calls.append(arguments)
            if arguments[:2] == ("debug", "prefs"):
                return unittest.mock.Mock(stdout=json.dumps({"ShieldsUp": prefs["shields"]}))
            if arguments[0] == "set":
                prefs["shields"] = True
                return unittest.mock.Mock(stdout="")
            if arguments[0] == "up":
                return unittest.mock.Mock(stdout="", stderr="visit https://login.tailscale.com/a/x")
            return unittest.mock.Mock(stdout=json.dumps({"BackendState": "Running", "Self": {"DNSName": "angi.ts.net."}}))
        prefs = {"shields": False}
        with patch.object(providers.tailnet, "run", side_effect=run), patch.object(providers.tailnet.Path, "exists", return_value=True):
            providers.tailnet.login()
            up = next(arguments for arguments in calls if arguments[0] == "up")
            self.assertIn("--shields-up=true", up)
            self.assertIn("--accept-dns=false", up)
            self.assertTrue(providers.tailnet.state()["ready"])  # lost shields were re-applied
            self.assertTrue(prefs["shields"])
            prefs["shields"] = False
            with patch.object(providers.tailnet, "shielded", return_value=False):
                state = providers.tailnet.state()
            self.assertFalse(state["ready"])  # never offered while incoming connections are possible
            self.assertIn("locked down", state["text"])

    def test_tailscale_state_describes_each_backend_state(self):
        def state(backend, **extra):
            with patch.object(providers.tailnet, "run", return_value=unittest.mock.Mock(stdout=json.dumps({"BackendState": backend, "ShieldsUp": True, **extra}))), patch.object(providers.tailnet.Path, "exists", return_value=True):
                return providers.tailnet.state()
        self.assertTrue(state("Running", Self={"DNSName": "angi.tail1234.ts.net."})["ready"])
        self.assertIn("angi.tail1234.ts.net", state("Running", Self={"DNSName": "angi.tail1234.ts.net."})["text"])
        self.assertEqual((state("NeedsLogin")["ready"], state("NeedsLogin")["login"]), (False, True))
        self.assertTrue(state("Stopped")["login"])
        self.assertFalse(state("Starting")["login"])


if __name__ == "__main__":
    unittest.main()
