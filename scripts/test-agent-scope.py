#!/usr/bin/env python3
"""OS and project scope: folder validation, agent prompts, launch directories and resume."""
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/agent"))
import chat
import main
import models
import scope
import session
import solo


class Sandbox:
    """A throwaway home, data volume and OS source standing in for the Pi's real locations."""
    def __enter__(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name).resolve()
        self.home, self.data = root / "home/dana", root / "data"
        self.source = self.data / "angi-dev"
        for path in (self.home, self.source, self.data / "agx-opencode-users/1000", self.data / "angi-sdk", self.data / "mine", self.data / "users/dana/work"):
            path.mkdir(parents=True)
        self.patches = [patch.object(scope.Path, "home", return_value=self.home), patch.object(scope, "DATA", self.data), patch.object(scope, "OS_SOURCE", self.source), patch.object(session, "SOURCE", self.source)]
        for item in self.patches:
            item.start()
        return self

    def __exit__(self, *_):
        for item in self.patches:
            item.stop()
        self.directory.cleanup()


class ValidationTests(unittest.TestCase):
    def rejects(self, path, fragment):
        with self.assertRaises(ValueError) as caught:
            scope.validate_project(path)
        self.assertIn(fragment, str(caught.exception), path)

    def test_a_folder_in_home_or_owned_on_the_data_volume_is_accepted_resolved(self):
        with Sandbox() as box:
            (box.home / "app").mkdir()
            self.assertEqual(scope.validate_project(box.home / "app"), box.home / "app")
            self.assertEqual(scope.validate_project(box.data / "mine"), box.data / "mine")
            self.assertEqual(scope.validate_project(box.data / "users/dana/work"), box.data / "users/dana/work")  # personal NVMe space
            (box.home / "alias").symlink_to(box.home / "app")
            self.assertEqual(scope.validate_project(box.home / "alias"), box.home / "app")  # the real folder, not the link
            self.assertEqual(scope.describe(box.home / "app"), {"path": str(box.home / "app"), "name": "app"})

    def test_missing_relative_and_file_paths_say_what_to_change(self):
        with Sandbox() as box:
            self.rejects("", "Enter a folder")
            self.rejects("   ", "Enter a folder")
            self.rejects("relative/dir", "full path")
            self.rejects(box.home / "nope", "does not exist")
            (box.home / "file.txt").write_text("x")
            self.rejects(box.home / "file.txt", "file, not a folder")

    def test_the_operating_system_source_system_folders_and_places_outside_the_user_are_refused(self):
        with Sandbox() as box:
            self.rejects(box.source, "OS mode")
            (box.source / "crates").mkdir()
            self.rejects(box.source / "crates", "OS mode")
            self.rejects(box.data / "agx-opencode-users/1000", "part of the system")  # OpenCode's credentials live here
            self.rejects(box.data / "angi-sdk", "part of the system")
            self.rejects(box.home, "inside it")
            self.rejects(box.data, "inside it")
            with patch.object(scope.os, "getuid", return_value=os.getuid() + 1):  # someone else's folder
                self.rejects(box.data / "users/dana/work", "own the folder")
            self.rejects(Path(box.directory.name).resolve(), "home or on the data volume")
            self.rejects("/", "home or on the data volume")
            self.rejects("/etc", "home or on the data volume")

    def test_links_cannot_smuggle_a_folder_in(self):
        with Sandbox() as box:
            outside = Path(box.directory.name).resolve() / "elsewhere"
            outside.mkdir()
            (box.home / "escape").symlink_to(outside)
            self.rejects(box.home / "escape", "home or on the data volume")
            (box.home / "to-os").symlink_to(box.source)
            self.rejects(box.home / "to-os", "OS mode")

    def test_hidden_settings_and_unwritable_folders_are_refused(self):
        with Sandbox() as box:
            for hidden in (".ssh", ".config/opencode"):
                (box.home / hidden).mkdir(parents=True)
                self.rejects(box.home / hidden, "hidden or a settings folder")
            (box.home / "readonly").mkdir()
            (box.home / "readonly").chmod(0o500)
            try:
                self.rejects(box.home / "readonly", "own the folder")
            finally:
                (box.home / "readonly").chmod(0o700)


class PromptTests(unittest.TestCase):
    def profiles(self, temporary):
        profiles = Path(temporary) / "profiles"
        profiles.mkdir(exist_ok=True)
        for role in models.ROLES:
            (profiles / f"{role}.md").write_text(f"ROLE {role}")
        return patch.object(session, "ASSETS", Path(temporary))

    def test_os_runs_are_limited_to_the_operating_system(self):
        text = scope.scope_text()
        self.assertIn("operating system", text)
        self.assertIn(str(scope.OS_SOURCE), text)
        self.assertIn("angi-root", text)

    def test_project_runs_name_the_folder_and_withhold_system_access(self):
        text = scope.scope_text(Path("/home/dana/shop"))
        self.assertIn("/home/dana/shop", text)
        self.assertIn("Do not use angi-root", text)
        self.assertIn("Do not modify the ANGI operating system", text)

    def test_every_role_gets_its_profile_then_the_scope_of_the_run(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_STATE_HOME": temporary}), self.profiles(temporary):
            os_run = json.loads(session.role_config())["agent"]
            project_run = json.loads(session.role_config(Path("/home/dana/shop")))["agent"]
        for role in models.ROLES:
            self.assertTrue(os_run[f"angi-{role}"]["prompt"].startswith(f"ROLE {role}"))
            self.assertIn("Scope: the ANGI operating system", os_run[f"angi-{role}"]["prompt"])
            self.assertIn("Scope: the user's project at /home/dana/shop", project_run[f"angi-{role}"]["prompt"])
            self.assertNotIn("operating system.\nWork only", project_run[f"angi-{role}"]["prompt"])

    def test_checked_in_profiles_do_not_assume_either_scope(self):
        for role in models.ROLES:
            text = (Path(__file__).resolve().parents[1] / f"apps/agent/profiles/{role}.md").read_text()
            self.assertNotIn("angi-root", text)
            self.assertNotIn("source checkout", text)


class LaunchTests(unittest.TestCase):
    def launch(self, box, project=None):
        calls = []
        def herdr(session_name, *arguments, **_):
            calls.append(arguments)
            if arguments[:2] == ("workspace", "create"):
                return {"root_pane": {"pane_id": "p1"}, "workspace": {"workspace_id": "w1"}}
            if arguments[:2] == ("pane", "split"):
                return {"pane": {"pane_id": f"p{len(calls)}"}}
            return {}
        state = Path(box.directory.name) / "state"
        state.mkdir()
        profiles = Path(box.directory.name) / "profiles"
        profiles.mkdir()
        for role in models.ROLES:
            (profiles / f"{role}.md").write_text(role)
        (box.source / "Cargo.toml").write_text("")
        environment = {"HERDR_CONFIG_PATH": str(Path(box.directory.name) / "herdr.toml"), "XDG_CONFIG_HOME": box.directory.name, "XDG_STATE_HOME": box.directory.name, "TMPDIR": box.directory.name}
        with patch.dict(os.environ, environment), patch.object(session, "ASSETS", Path(box.directory.name)), patch.object(session, "herdr", side_effect=herdr), \
                patch.object(session, "configure"), patch.object(session, "start_server"), patch.object(session, "ensure_dispatcher"), patch.object(session.os, "execvp"):
            session.launch(state, "build it", project)
            manifest = json.loads(next(state.glob("angi-*/run.json")).read_text())
            config = json.loads(os.environ["OPENCODE_CONFIG_CONTENT"])
        return calls, manifest, config, state

    def directories(self, calls):
        found = []
        for arguments in calls:
            if "--cwd" in arguments:
                found.append(arguments[arguments.index("--cwd") + 1])
        return found

    def test_os_runs_start_every_pane_in_the_operating_system_source(self):
        with Sandbox() as box:
            calls, manifest, config, _ = self.launch(box)
            self.assertEqual(set(self.directories(calls)), {str(box.source)})
            self.assertEqual(len(self.directories(calls)), 3)
            self.assertEqual((manifest["mode"], manifest["project"]), ("os", None))
            self.assertIn("ANGI workshop", next(arguments for arguments in calls if arguments[:2] == ("workspace", "create")))
            self.assertIn("Scope: the ANGI operating system", config["agent"]["angi-builder"]["prompt"])

    def test_project_runs_start_every_pane_in_the_chosen_folder_and_record_it(self):
        with Sandbox() as box:
            (box.home / "shop").mkdir()
            calls, manifest, config, _ = self.launch(box, box.home / "shop")
            self.assertEqual(set(self.directories(calls)), {str(box.home / "shop")})
            self.assertEqual(len(self.directories(calls)), 3)
            self.assertEqual((manifest["mode"], manifest["project"]), ("project", str(box.home / "shop")))
            self.assertIn("Project: shop", next(arguments for arguments in calls if arguments[:2] == ("workspace", "create")))
            self.assertIn("Scope: the user's project", config["agent"]["angi-orchestrator"]["prompt"])

    def test_launch_refuses_a_folder_that_fails_validation_before_starting_anything(self):
        with Sandbox() as box:
            with self.assertRaises(ValueError):
                self.launch(box, box.source)

    def test_resume_keeps_the_scope_a_run_started_with(self):
        with Sandbox() as box:
            (box.home / "shop").mkdir()
            _, _, _, state = self.launch(box, box.home / "shop")
            seen = {}
            with patch.dict(os.environ, {"XDG_CONFIG_HOME": box.directory.name}), patch.object(session, "ASSETS", Path(box.directory.name)), patch.object(session, "configure"), \
                    patch.object(session, "start_server"), patch.object(session, "ensure_dispatcher"), patch.object(session.os, "execvp"):
                session.resume(state)
                seen["project"] = json.loads(os.environ["OPENCODE_CONFIG_CONTENT"])["agent"]["angi-tester"]["prompt"]
                run = next(state.glob("angi-*/run.json"))
                legacy = json.loads(run.read_text())
                del legacy["mode"], legacy["project"]  # a run recorded before project mode existed
                run.write_text(json.dumps(legacy))
                session.resume(state)
                seen["legacy"] = json.loads(os.environ["OPENCODE_CONFIG_CONTENT"])["agent"]["angi-tester"]["prompt"]
            self.assertIn("project at " + str(box.home / "shop"), seen["project"])
            self.assertIn("Scope: the ANGI operating system", seen["legacy"])


class ChatTests(unittest.TestCase):
    """Chat uses one agent with Angi's persona and bounded file tools."""
    def assets(self, temporary):
        (Path(temporary) / "profiles").mkdir(exist_ok=True)
        (Path(temporary) / "profiles/chat.md").write_text("ROLE chat")
        skill = Path(temporary) / "skills/angi-chat"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text("---\nname: angi-chat\n---\nTalk like Angi.")
        return patch.object(session, "ASSETS", Path(temporary))

    def test_chat_can_use_home_without_changing_what_project_accepts(self):
        with Sandbox() as box:
            with self.assertRaisesRegex(ValueError, "not the whole location"):
                scope.validate_project(box.home)
            self.assertEqual(scope.validate_project(box.home, allow_home=True), box.home)

    def test_chat_is_one_agent_with_its_persona_in_the_prompt(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_STATE_HOME": temporary}), self.assets(temporary):
            config = json.loads(chat.config(Path(temporary)))
            agent = config["agent"]
        self.assertEqual(list(agent), ["angi-chat"])
        self.assertEqual(agent["angi-chat"]["permission"], {
            "*": "deny", "angi_files_list": "allow", "angi_files_read": "allow",
            "angi_files_mkdir": "allow", "angi_files_save": "allow",
            # ANGI's own memory, kept in its knowledge graph: nothing else beyond files is allowed.
            "angi_memory_remember": "allow", "angi_memory_forget": "allow", "angi_memory_search": "allow",
        })
        self.assertEqual(set(config["mcp"]), {"angi_files", "angi_memory"})
        self.assertTrue(agent["angi-chat"]["prompt"].startswith("ROLE chat"))
        self.assertIn(temporary, agent["angi-chat"]["prompt"])
        self.assertIn("Talk like Angi.", agent["angi-chat"]["prompt"])

    def test_the_persona_skill_and_profile_ship_with_the_agent(self):
        root = Path(__file__).resolve().parents[1] / "apps/agent"
        skill = (root / "skills/angi-chat/SKILL.md").read_text()
        self.assertTrue(skill.startswith("---\nname: angi-chat\n"))
        self.assertIn("Angi", skill)
        self.assertIn("file tools", (root / "profiles/chat.md").read_text())
        for forbidden in ("angi-root", "angi-update"):
            self.assertNotIn(forbidden, (root / "profiles/chat.md").read_text())

    def test_a_chat_starts_in_its_own_folder_and_is_recorded_as_a_chat(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"HOME": temporary, "XDG_CONFIG_HOME": temporary, "XDG_STATE_HOME": temporary}), self.assets(temporary), \
                patch.object(chat.os, "chdir") as chdir, patch.object(chat.os, "execvp") as execvp:
            state = Path(temporary) / "state"
            state.mkdir()
            chat.launch(state, "hi Angi")
            run = json.loads(next(state.glob("angi-chat-*/run.json")).read_text())
            execvp.assert_called_once_with("opencode", ["opencode", "--agent", "angi-chat", "--prompt", "hi Angi"])
            chdir.assert_called_once_with(state / "chat")
            self.assertEqual((run["mode"], run["brief"]), ("chat", "hi Angi"))
            self.assertTrue(chat.newest(state))
            self.assertFalse(solo.newest(state))

    def test_replies_are_the_text_parts_or_the_error(self):
        text = lambda value: json.dumps({"type": "text", "part": {"text": value}})
        self.assertEqual(chat.parse_reply("\n".join([json.dumps({"type": "step_start"}), text("Hey "), "noise", text("there!")])), ("Hey there!", None, None))
        failure = json.dumps({"type": "error", "sessionID": "ses_declined", "error": {"name": "APIError", "data": {"message": "No credits"}}})
        self.assertEqual(chat.parse_reply(failure), ("", "No credits", "ses_declined"))

    def test_a_turn_continues_the_chat_unless_it_is_new_and_reports_failures(self):
        reply = lambda out, err="": subprocess.CompletedProcess([], 0, out, err)
        said = json.dumps({"type": "text", "part": {"text": "Hi!"}})
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"HOME": temporary, "XDG_CONFIG_HOME": temporary, "XDG_STATE_HOME": temporary}), self.assets(temporary), \
                patch.object(chat.os, "chdir"):
            state = Path(temporary)
            with patch.object(chat.subprocess, "run", return_value=reply(said)) as run:
                self.assertEqual(chat.turn(state, "hello"), {"text": "Hi!"})
                config = json.loads(os.environ["OPENCODE_CONFIG_CONTENT"])
                self.assertEqual(config["mcp"]["angi_files"]["command"][-1], str(state.resolve()))
                self.assertIn("--continue", run.call_args.args[0])
                chat.turn(state, "hello", new=True)
                self.assertNotIn("--continue", run.call_args.args[0])
                self.assertEqual(run.call_args.args[0][-1], "hello")
            with patch.object(chat.subprocess, "run", return_value=reply("", "boom\nModel not found")):
                self.assertEqual(chat.turn(state, "hello"), {"error": "Model not found"})
            with patch.object(chat.subprocess, "run", side_effect=subprocess.TimeoutExpired("opencode", 1)):
                self.assertIn("too long", chat.turn(state, "hello")["error"])

    def test_resume_follows_the_most_recent_kind_of_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary)
            self.assertFalse(chat.newest(state))
            for name, when in (("current", 100), ("os-current", 200), ("chat-current", 300)):
                (state / name).write_text("x")
                os.utime(state / name, (when, when))
            self.assertTrue(chat.newest(state))
            self.assertFalse(solo.newest(state))
            os.utime(state / "os-current", (400, 400))
            self.assertFalse(chat.newest(state))
            self.assertTrue(solo.newest(state))


class CommandTests(unittest.TestCase):
    def test_angi_request_forms(self):
        self.assertEqual(main.angi_request(["angi"]), (None, None, False))
        self.assertEqual(main.angi_request(["angi", "--prompt", " build a thing "]), ("build a thing", None, False))
        self.assertEqual(main.angi_request(["angi", "--prompt", "build", "--project", "/home/dana/shop"]), ("build", "/home/dana/shop", False))
        self.assertEqual(main.angi_request(["angi", "--prompt", " hi Angi ", "--chat"]), ("hi Angi", None, True))
        for bad in ([], ["angi", "--prompt"], ["angi", "--prompt", ""], ["angi", "--prompt", "x", "--project"], ["angi", "--prompt", "x", "--other", "y"], ["angi", "--prompt", "x", "--project", " "], ["angi", "--prompt", "x", "--chat", "y"], ["angi", "--prompt", "x", "--project", "/a", "--chat"], ["status"]):
            self.assertIs(main.angi_request(bad), False, bad)

    def test_project_check_reports_a_path_or_an_error_as_one_json_line(self):
        def ask(path):
            output = io.StringIO()
            with patch.object(sys, "argv", ["agent", "project", path]), redirect_stdout(output):
                main.main()
            return json.loads(output.getvalue())
        with Sandbox() as box:
            (box.home / "shop").mkdir()
            self.assertEqual(ask(str(box.home / "shop")), {"path": str(box.home / "shop"), "name": "shop"})
            self.assertIn("OS mode", ask(str(box.source))["error"])
            self.assertIn("does not exist", ask(str(box.home / "missing"))["error"])


if __name__ == "__main__":
    unittest.main()
