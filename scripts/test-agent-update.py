#!/usr/bin/env python3
"""angi-update: the release ledger, the install guard against reverting to older source, and rollback."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/agent"))
# Recording a release or an app in ANGI's knowledge graph must not touch this account's real one.
os.environ['XDG_STATE_HOME'] = tempfile.mkdtemp(prefix='angi-test-state-')
import main
import models
import refresh
import releases
import session
import solo
import update


def run(*command, cwd):
    subprocess.run(command, cwd=cwd, check=True, capture_output=True, env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"})


class Device:
    """A throwaway release directory, /usr/bin link and source checkout standing in for the Pi."""
    def __enter__(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name).resolve()
        self.gui, self.link, self.source = root / "agx-gui", root / "usr-bin-agx-gui", root / "angi-dev"
        self.gui.mkdir()
        self.source.mkdir()
        run("git", "init", "-q", cwd=self.source)
        (self.source / "app.slint").write_text("one\n")
        run("git", "add", "-A", cwd=self.source)
        run("git", "commit", "-qm", "base", cwd=self.source)
        self.patches = [patch.object(releases, "GUI_DIR", self.gui), patch.object(releases, "LEDGER", self.gui / "ledger.json"),
                        patch.object(releases, "LINK", self.link), patch.object(releases, "SOURCE", self.source), patch.object(releases, "ROOT", [])]
        for item in self.patches:
            item.start()
        return self

    def __exit__(self, *_):
        for item in self.patches:
            item.stop()
        self.directory.cleanup()

    def release(self, name, content):
        (self.gui / name).write_bytes(content)
        return name

    def externally_installed(self, name, content=b"other"):
        """A release put in place by something other than angi-update, e.g. an agent's own deploy."""
        self.release(name, content)
        if self.link.is_symlink():
            self.link.unlink()
        self.link.symlink_to(self.gui / name)

    def build(self, content):
        binary = self.gui / "built"
        binary.write_bytes(content)
        return lambda: binary

    def trusted_baseline(self):
        """The workstation deploy: install a release, sync its source and record it."""
        self.externally_installed("agx-gui.base0000000000000", b"base")
        archive = self.gui / "src.tar"
        subprocess.run(["tar", "-cf", str(archive), "-C", str(self.source), "app.slint"], check=True)
        releases.adopt("agx-gui.base0000000000000", "workstation", releases.tree_hash(), None, archive, f"{os.getuid()}:{os.getgid()}")


class GuardTests(unittest.TestCase):
    def test_an_installed_release_without_a_source_record_blocks_installs(self):
        with Device() as device:
            device.externally_installed("agx-gui.unrecorded")
            with self.assertRaisesRegex(releases.Refused, "no source record"):
                releases.install(device.build(b"new"))
            self.assertEqual(releases.installed(), "agx-gui.unrecorded")

    def test_a_release_installed_behind_the_checkouts_back_blocks_installs(self):
        # The incident this guard exists for: a newer release is installed from elsewhere, then an agent
        # builds from its stale checkout. The stale build must not replace it.
        with Device() as device:
            device.trusted_baseline()
            device.externally_installed("agx-gui.otherbuild00000", b"workstation")
            releases.write_ledger([*releases.read_ledger(), {"release": "agx-gui.otherbuild00000", "origin": "workstation", "tree": "x", "parent": None}])
            with self.assertRaisesRegex(releases.Refused, "make deploy-gui"):
                releases.install(device.build(b"stale"))
            self.assertEqual(releases.installed(), "agx-gui.otherbuild00000")
            self.assertFalse((device.gui / releases.release_name(device.gui / "built")).exists())

    def test_status_reports_why_an_install_is_blocked(self):
        with Device() as device:
            device.externally_installed("agx-gui.unrecorded")
            state = releases.status()
            self.assertFalse(state["can_install"])
            self.assertIn("no source record", state["reason"])


class InstallTests(unittest.TestCase):
    def test_install_records_source_switches_the_link_and_keeps_the_previous_release(self):
        with Device() as device:
            device.trusted_baseline()
            (device.source / "app.slint").write_text("two\n")
            message = releases.install(device.build(b"feature"))
            name = releases.release_name(device.gui / "built")
            self.assertEqual(releases.installed(), name)
            self.assertIn("Installed " + name, message)
            self.assertTrue((device.gui / "agx-gui.base0000000000000").exists())
            known = releases.entry(name)
            self.assertEqual((known["origin"], known["parent"]), ("pi", "agx-gui.base0000000000000"))
            self.assertEqual(known["tree"], releases.tree_hash())
            self.assertEqual(releases.base(), name)
            # The release's exact source stays recoverable from the checkout's history.
            ref = releases.git("rev-parse", f"refs/angi/{name}^{{tree}}")
            self.assertEqual(ref, known["tree"])

    def test_installs_chain_so_the_next_update_is_allowed(self):
        with Device() as device:
            device.trusted_baseline()
            (device.source / "app.slint").write_text("two\n")
            releases.install(device.build(b"first"))
            (device.source / "app.slint").write_text("three\n")
            releases.install(device.build(b"second"))
            self.assertEqual(releases.installed(), releases.release_name(device.gui / "built"))
            self.assertEqual(len(releases.read_ledger()), 3)

    def test_the_same_build_is_not_installed_twice(self):
        with Device() as device:
            device.trusted_baseline()
            releases.install(device.build(b"same"))
            self.assertIn("Already installed", releases.install(device.build(b"same")))
            self.assertEqual(len(releases.read_ledger()), 2)

    def test_edits_during_a_build_are_refused_rather_than_mislabelled(self):
        with Device() as device:
            device.trusted_baseline()
            build = device.build(b"racing")
            def edit_then_build():
                (device.source / "app.slint").write_text("changed mid-build\n")
                return build()
            with self.assertRaisesRegex(releases.Refused, "changed while it was building"):
                releases.install(edit_then_build)
            self.assertEqual(releases.installed(), "agx-gui.base0000000000000")

    def test_rollback_restores_the_previous_release_and_keeps_installs_possible(self):
        with Device() as device:
            device.trusted_baseline()
            releases.install(device.build(b"bad"))
            self.assertIn("Restored agx-gui.base0000000000000", releases.rollback())
            self.assertEqual(releases.installed(), "agx-gui.base0000000000000")
            self.assertTrue(releases.status()["can_install"])

    def test_rollback_without_an_earlier_release_is_refused(self):
        with Device() as device:
            device.trusted_baseline()
            with self.assertRaisesRegex(releases.Refused, "no earlier release"):
                releases.rollback()


class BuildRecoveryTests(unittest.TestCase):
    """A build cut off partway leaves an incremental cache that fails to link; the updater repairs that itself."""
    LINKER = "error: linking with `cc` failed\n  undefined reference to `core::ptr::drop_glue::<Action>`\n"

    def run_build(self, results):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory) / "incremental"
            (cache / "agx_gui-1").mkdir(parents=True)
            calls = []
            def compile():
                calls.append(cache.exists())
                return results[len(calls) - 1]
            try:
                update.build(compile, cache)
                error = None
            except releases.Refused as refused:
                error = str(refused)
            return calls, error

    def test_a_link_failure_is_retried_once_from_a_clean_cache(self):
        calls, error = self.run_build([(1, self.LINKER), (0, "Finished\n")])
        self.assertEqual(calls, [True, False])
        self.assertIsNone(error)

    def test_a_link_failure_that_persists_is_reported_after_one_retry(self):
        calls, error = self.run_build([(1, self.LINKER), (1, self.LINKER)])
        self.assertEqual(len(calls), 2)
        self.assertIn("did not build", error)

    def test_ordinary_compile_errors_are_not_retried(self):
        calls, error = self.run_build([(1, "error[E0425]: cannot find value `x` in this scope\n")])
        self.assertEqual(calls, [True])
        self.assertIn("did not build", error)

    def test_a_good_build_is_left_alone(self):
        calls, error = self.run_build([(0, "Finished\n")])
        self.assertEqual(calls, [True])
        self.assertIsNone(error)


class RefreshTests(unittest.TestCase):
    """Loading a release into the running desktop closes the agent's own terminal, so it is guarded."""
    NEW, OLD = "agx-gui.new0000000000000", "agx-gui.base0000000000000"

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.device = Device().__enter__()
        self.addCleanup(self.device.__exit__)
        self.device.trusted_baseline()
        (self.device.gui / self.NEW).write_bytes(b"\x7fELF new")
        (self.device.gui / self.NEW).chmod(0o755)
        releases.write_ledger([*releases.read_ledger(), {"release": self.NEW, "origin": "pi", "tree": "t", "parent": self.OLD}])
        self.device.link.unlink()
        self.device.link.symlink_to(self.device.gui / self.NEW)
        (self.device.source / ".git/angi-base").write_text(self.NEW + "\n")
        self.result = Path(self.directory.name) / "refresh.json"
        self.patches = [patch.object(refresh, "RESULT", self.result), patch.object(refresh, "DELAY", 0)]
        for item in self.patches:
            item.start()
        self.addCleanup(lambda: [item.stop() for item in self.patches])

    def test_it_schedules_a_detached_watchdog_for_a_desktop_that_can_take_the_signal(self):
        with patch.object(refresh, "desktops", return_value={77: self.OLD}), patch.object(refresh, "handles_refresh_signal", return_value=True), \
                patch.object(refresh.subprocess, "Popen") as spawn:
            message = refresh.start()
        arguments = spawn.call_args
        self.assertEqual(arguments.args[0][-4:], ["watch", "77", self.NEW, self.OLD])
        self.assertTrue(arguments.kwargs["start_new_session"])
        self.assertIn("finish your report", message)

    def test_a_desktop_without_the_signal_handler_is_never_signalled(self):
        # SIGUSR1 would terminate it and could drop the session to login.
        with patch.object(refresh, "desktops", return_value={77: self.OLD}), patch.object(refresh, "handles_refresh_signal", return_value=False), \
                patch.object(refresh.subprocess, "Popen") as spawn:
            with self.assertRaisesRegex(releases.Refused, "Settings > Refresh desktop"):
                refresh.start()
        spawn.assert_not_called()

    def test_nothing_happens_when_no_desktop_runs_or_it_already_runs_the_release(self):
        with patch.object(refresh, "desktops", return_value={}), self.assertRaisesRegex(releases.Refused, "nothing to refresh"):
            refresh.start()
        with patch.object(refresh, "desktops", return_value={77: self.NEW}), patch.object(refresh.subprocess, "Popen") as spawn:
            self.assertIn("already runs", refresh.start())
        spawn.assert_not_called()

    def test_a_release_not_installed_by_the_updater_is_not_refreshed(self):
        self.device.externally_installed("agx-gui.unrecorded")
        with patch.object(refresh, "desktops", return_value={77: self.OLD}), self.assertRaisesRegex(releases.Refused, "no source record"):
            refresh.start()

    def watch(self, states):
        """Run the watchdog against a desktop whose process table is `states`, one entry per second."""
        clock = {"now": 0.0}
        def sleep(seconds):
            clock["now"] += seconds
        def desktops():
            return states(clock["now"])
        with patch.object(refresh, "desktops", desktops), patch.object(refresh.os, "kill") as kill:
            outcome = refresh.watch(77, self.NEW, self.OLD, sleep, lambda: clock["now"])
        return outcome, kill, json.loads(self.result.read_text())

    def test_a_desktop_that_loads_the_release_and_stays_up_is_a_success(self):
        outcome, kill, saved = self.watch(lambda t: {77: self.OLD if t < 5 else self.NEW})
        kill.assert_called_once_with(77, refresh.signal.SIGUSR1)
        self.assertEqual(outcome, "ok")
        self.assertEqual(releases.installed(), self.NEW)

    def test_a_desktop_that_does_not_restart_keeps_the_release_and_says_so(self):
        outcome, _, saved = self.watch(lambda t: {77: self.OLD})
        self.assertEqual(outcome, "not-refreshed")
        self.assertEqual(releases.installed(), self.NEW)

    def test_a_desktop_that_vanishes_is_rolled_back_to_the_previous_release(self):
        outcome, _, saved = self.watch(lambda t: {77: self.OLD} if t < 5 else {})
        self.assertEqual(outcome, "rolled-back")
        self.assertEqual(releases.installed(), self.OLD)
        self.assertIn(self.OLD, saved["detail"])

    def test_a_new_desktop_that_crashes_soon_after_loading_is_rolled_back(self):
        outcome, _, _ = self.watch(lambda t: {77: self.NEW} if t < 10 else {})
        self.assertEqual(outcome, "rolled-back")
        self.assertEqual(releases.installed(), self.OLD)


class SyncTests(unittest.TestCase):
    def test_a_workstation_sync_keeps_the_checkouts_own_edits_in_history(self):
        with Device() as device:
            (device.source / "app.slint").write_text("agent edit not on the workstation\n")
            (device.source / "stale.rs").write_text("deleted on the workstation\n")
            device.externally_installed("agx-gui.workstation0000", b"ws")
            incoming = Path(device.directory.name) / "incoming"
            incoming.mkdir()
            (incoming / "app.slint").write_text("workstation version\n")
            archive = Path(device.directory.name) / "incoming.tar"
            subprocess.run(["tar", "-cf", str(archive), "-C", str(incoming), "app.slint"], check=True)
            run("git", "init", "-q", cwd=incoming)
            wanted = releases.tree_hash(incoming)
            releases.adopt("agx-gui.workstation0000", "workstation", wanted, None, archive, f"{os.getuid()}:{os.getgid()}")
            self.assertEqual((device.source / "app.slint").read_text(), "workstation version\n")
            self.assertFalse((device.source / "stale.rs").exists())
            self.assertEqual(releases.base(), "agx-gui.workstation0000")
            kept = releases.git("for-each-ref", "--format=%(refname)", "refs/angi/pi-edits-*").split()
            self.assertEqual(len(kept), 1)
            self.assertEqual(releases.git("show", kept[0] + ":stale.rs"), "deleted on the workstation")
            self.assertIsNone(releases.guard())

    def test_a_sync_that_does_not_reproduce_the_source_is_not_recorded(self):
        with Device() as device:
            device.externally_installed("agx-gui.workstation0000", b"ws")
            archive = Path(device.directory.name) / "src.tar"
            subprocess.run(["tar", "-cf", str(archive), "-C", str(device.source), "app.slint"], check=True)
            with self.assertRaisesRegex(releases.Refused, "does not match"):
                releases.adopt("agx-gui.workstation0000", "workstation", "0" * 40, None, archive, f"{os.getuid()}:{os.getgid()}")
            self.assertIsNone(releases.entry("agx-gui.workstation0000"))


class SoloTests(unittest.TestCase):
    """OS updates are one OpenCode agent with its own model; Herdr stays for projects."""
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        (root / "profiles").mkdir()
        (root / "profiles/os.md").write_text("OS PROFILE")
        (root / "skills/angi-gui").mkdir(parents=True)
        (root / "skills/angi-gui/SKILL.md").write_text("gui skill")
        self.source = root / "angi-dev"
        self.source.mkdir()
        (self.source / "Cargo.toml").write_text("")
        self.state = root / "state"
        self.state.mkdir()
        self.environment = patch.dict(os.environ, {"XDG_STATE_HOME": str(self.state), "XDG_CONFIG_HOME": str(root / "config")})
        self.patches = [self.environment, patch.object(session, "ASSETS", root), patch.object(solo, "OS_SOURCE", self.source), patch.object(solo.os, "chdir")]
        for item in self.patches:
            item.start()
        self.addCleanup(lambda: [item.stop() for item in self.patches] and self.directory.cleanup())

    def test_the_os_agent_is_alone_and_uses_the_model_chosen_for_os_updates(self):
        with patch.object(solo, "active_selections", return_value={"os": "openai/fast", "builder": "openai/slow"}), patch.object(session, "active", return_value=None):
            agents = json.loads(solo.config())["agent"]
        self.assertEqual(list(agents), ["angi-os"])
        self.assertEqual(agents["angi-os"]["model"], "openai/fast")
        self.assertTrue(agents["angi-os"]["prompt"].startswith("OS PROFILE"))
        self.assertIn("angi-update", agents["angi-os"]["prompt"])

    def test_without_an_os_model_opencode_picks_its_default(self):
        with patch.object(solo, "active_selections", return_value={"builder": "openai/slow"}), patch.object(session, "active", return_value=None):
            self.assertNotIn("model", json.loads(solo.config())["agent"]["angi-os"])

    def test_an_os_run_starts_opencode_directly_without_herdr(self):
        with patch.object(solo, "active_selections", return_value={}), patch.object(session, "active", return_value=None), \
                patch.object(session, "herdr", side_effect=AssertionError("Herdr must not run for OS updates")), patch.object(solo.os, "execvp") as execute:
            solo.launch(self.state, "add a happy face")
        execute.assert_called_once_with("opencode", ["opencode", "--agent", "angi-os", "--prompt", "add a happy face"])
        self.assertTrue((self.state / "os-current").exists())
        self.assertEqual((Path(os.environ["XDG_CONFIG_HOME"]) / "opencode/skills/angi-gui/SKILL.md").read_text(), "gui skill")

    def test_launch_routes_os_and_project_runs_to_the_swarm(self):
        with patch.object(main.sys, "argv", ["agent", "angi", "--prompt", "x"]), patch.object(main.os, "getuid", return_value=1000), \
                patch.object(main, "solo") as os_run, patch.object(main, "launch") as swarm:
            main.main()
            swarm.assert_called_once_with(self.state / "angi-swarm", "x", None)
            os_run.launch.assert_not_called()
        with patch.object(main.sys, "argv", ["agent", "angi", "--prompt", "x", "--project", "/home/dana/shop"]), patch.object(main.os, "getuid", return_value=1000), \
                patch.object(main, "solo") as os_run, patch.object(main, "launch") as swarm:
            main.main()
            swarm.assert_called_once_with(self.state / "angi-swarm", "x", "/home/dana/shop")
            os_run.launch.assert_not_called()

    def test_resume_follows_whichever_run_is_newest(self):
        (self.state / "current").write_text("angi-old")
        os.utime(self.state / "current", (1, 1))
        (self.state / "os-current").write_text("angi-os-new")
        self.assertTrue(solo.newest(self.state))
        os.utime(self.state / "os-current", (1, 1))
        os.utime(self.state / "current", (5, 5))
        self.assertFalse(solo.newest(self.state))


class OsModelTests(unittest.TestCase):
    def test_a_model_can_be_chosen_for_os_updates_and_is_kept_with_the_others(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"XDG_STATE_HOME": directory}), \
                patch.object(models, "active", return_value="openai"), patch.object(models, "catalog", return_value=["openai/fast", "openai/slow"]):
            models.save("orchestrator", "openai/slow")
            self.assertEqual(models.save("os", "openai/fast"), {"orchestrator": "openai/slow", "os": "openai/fast"})
            self.assertEqual(models.selections("openai")["os"], "openai/fast")
            with self.assertRaises(ValueError):
                models.save("deployer", "openai/fast")

    def test_the_swarm_roles_do_not_include_the_os_role(self):
        self.assertNotIn("os", models.ROLES)
        self.assertIn("os", models.ALL_ROLES)


if __name__ == "__main__":
    unittest.main()
