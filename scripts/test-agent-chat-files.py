#!/usr/bin/env python3
"""Exercise the Chat tool protocol against a disposable local folder."""
from contextlib import closing
import json
from http.client import HTTPConnection
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SERVER = Path(__file__).resolve().parents[1] / "apps/agent/chat_files.py"
sys.path.insert(0, str(SERVER.parent))
import agent_preview
import chat_files
import work_changes


class ChatFileTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.home = Path(temporary.name).resolve()
        self.folder = self.home / "selected folder"
        self.folder.mkdir()
        self.client = subprocess.Popen([sys.executable, str(SERVER), str(self.folder)],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
                                       env={**os.environ, "HOME": str(self.home)})
        self.addCleanup(self.close_client)

    def close_client(self):
        self.client.stdin.close()
        self.client.stdout.close()
        self.client.wait(timeout=3)
        self.assertEqual(self.client.returncode, 0)

    def request(self, method, **params):
        request = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
        print(json.dumps(request), file=self.client.stdin, flush=True)
        return json.loads(self.client.stdout.readline())["result"]

    def call(self, name, **arguments):
        return self.request("tools/call", name=name, arguments=arguments)

    def test_mcp_creates_directories_and_saves_and_reads_utf8_text(self):
        self.assertIn("tools", self.request("initialize")["capabilities"])
        names = {tool["name"] for tool in self.request("tools/list")["tools"]}
        self.assertEqual(names, {"mkdir", "save", "list", "read", "preview"})
        self.assertFalse(self.call("mkdir", path="notes/empty")["isError"])
        self.assertTrue((self.folder / "notes/empty").is_dir())
        self.assertFalse(self.call("save", path="notes/hello.txt", text="Café\n")["isError"])
        self.assertEqual((self.folder / "notes/hello.txt").read_text(), "Café\n")
        result = json.loads(self.call("list", path="notes")["content"][0]["text"])
        self.assertEqual(result["entries"], ["empty/", "hello.txt"])
        self.assertEqual(self.call("read", path="notes/hello.txt")["content"][0]["text"], "Café\n")

    def test_refuses_paths_outside_the_selected_folder_and_link_targets(self):
        outside = self.home / "outside"
        outside.mkdir()
        secret = outside / "file.txt"
        secret.write_text("original")
        (self.folder / "linked").symlink_to(outside, target_is_directory=True)
        (self.folder / "linked.txt").symlink_to(secret)
        (self.folder / ".private").write_text("private")
        for operation, arguments in (
            ("mkdir", {"path": "../outside/escape"}),
            ("save", {"path": str(secret), "text": "new", "overwrite": True}),
            ("save", {"path": "linked/file.txt", "text": "new", "overwrite": True}),
            ("save", {"path": "linked.txt", "text": "new", "overwrite": True}),
            ("read", {"path": "linked.txt"}),
            ("list", {"path": "linked"}),
            ("read", {"path": ".private"}),
            ("save", {"path": ".private", "text": "new", "overwrite": True}),
            ("preview", {"path": "../outside"}),
            ("preview", {"path": "linked"}),
        ):
            with self.subTest(operation=operation, arguments=arguments):
                self.assertTrue(self.call(operation, **arguments)["isError"])
        self.assertEqual(secret.read_text(), "original")
        self.assertFalse((outside / "escape").exists())

    def test_overwrite_must_be_explicit(self):
        path = self.folder / "file.txt"
        path.write_text("original")
        self.assertTrue(self.call("save", path="file.txt", text="new")["isError"])
        self.assertEqual(path.read_text(), "original")
        self.assertFalse(self.call("save", path="file.txt", text="new", overwrite=True)["isError"])
        self.assertEqual(path.read_text(), "new")
        self.assertEqual(list(self.folder.iterdir()), [path])

    def test_successful_html_save_and_show_request_open_the_same_file(self):
        fd = os.open(self.folder, chat_files.DIRECTORY)
        self.addCleanup(os.close, fd)
        def call(name, **arguments):
            return chat_files.respond(fd, {"id": 1, "method": "tools/call", "params": {
                "name": name, "arguments": arguments,
            }}, show_saved=True)["result"]

        with patch.object(agent_preview, "request", return_value="Preview requested.") as request:
            self.assertFalse(call("save", path="index.html", text="<h1>App</h1>")["isError"])
            request.assert_called_once_with(self.folder / "index.html")
            self.assertTrue(call("save", path="index.html", text="refused")["isError"])
            request.assert_called_once()
            request.reset_mock()
            with patch.object(agent_preview.Path, "home", return_value=self.home):
                self.assertFalse(call("preview")["isError"])
            request.assert_called_once_with(self.folder / "index.html")

    def test_finished_work_previews_changed_html_and_prefers_its_index(self):
        pages = self.folder / "site"
        pages.mkdir()
        for name in ("index.html", "about.html"):
            (pages / name).write_text("<h1>Site</h1>")
        self.assertEqual(agent_preview.entry_point(pages), pages / "index.html")
        run = self.home / "run"
        run.mkdir()
        work_changes.write_record(run / work_changes.CHANGES, {"root": str(self.folder), "files": {
            "site/about.html": {"kind": "Modified"}, "site/index.html": {"kind": "Added"},
            "old.html": {"kind": "Deleted"},
        }})
        with patch.object(agent_preview, "request") as request:
            agent_preview.after_report(run)
            request.assert_called_once_with(pages / "index.html")
            (run / "preview-shown.json").write_text("{}")
            agent_preview.after_report(run)
            request.assert_called_once()

    def test_chat_turn_shows_the_index_after_the_assets_are_saved(self):
        receipt = self.home / "receipt"
        (self.folder / "index.html").touch()
        (self.folder / "about.html").touch()
        with patch.dict(os.environ, {"AGX_CHAT_PREVIEW_QUEUE": str(receipt)}), patch.object(agent_preview, "request") as request:
            agent_preview.saved_in_chat(self.folder, "about.html")
            agent_preview.saved_in_chat(self.folder, "index.html")
            request.assert_not_called()
            (self.folder / "app.js").touch()
            with receipt.open() as queued:
                agent_preview.show_chat_saves(queued)
            request.assert_called_once_with(self.folder / "index.html")


class PreviewHttpTests(unittest.TestCase):
    def test_preview_serves_assets_with_a_session_cookie_and_refuses_other_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "index.html").write_text('<link rel="stylesheet" href="style.css"><h1>Ready</h1>')
            (root / "style.css").write_text("h1 { color: green }")
            (root / ".secret.json").write_text("secret")
            (root / "link.css").symlink_to(root / "style.css")
            (root / "empty").mkdir()
            server = subprocess.Popen([sys.executable, "-I", "-u", str(SERVER.with_name("preview_http.py")), folder],
                                      stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                                      env={**os.environ, "AGX_PREVIEW_TOKEN": "test-token"})
            try:
                port = int(server.stdout.readline())
                with closing(HTTPConnection("127.0.0.1", port, timeout=3)) as client:
                    def get(path, headers=None):
                        client.request("GET", path, headers=headers or {})
                        response = client.getresponse()
                        return response.status, response.headers, response.read().decode()

                    self.assertEqual(get("/index.html")[0], 403)
                    status, headers, _ = get("/index.html?angi_preview=test-token")
                    self.assertEqual((status, headers["Location"]), (303, "/index.html"))
                    cookie = {"Cookie": headers["Set-Cookie"].split(";", 1)[0]}
                    self.assertIn("<h1>Ready</h1>", get("/index.html", cookie)[2])
                    status, headers, content = get("/style.css", cookie)
                    self.assertEqual((status, headers["Content-Type"], content), (200, "text/css", "h1 { color: green }"))
                    self.assertEqual(get("/style.css", cookie | {"Sec-Fetch-Site": "cross-site"})[0], 403)
                    for path in ("/link.css", "/%2Esecret.json", "/%2E%2E/style.css", "/empty/"):
                        self.assertEqual(get(path, cookie)[0], 404, path)
            finally:
                server.terminate()
                server.wait(timeout=3)
                server.stdout.close()


if __name__ == "__main__":
    unittest.main()
