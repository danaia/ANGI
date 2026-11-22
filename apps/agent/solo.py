"""OS updates: one OpenCode agent, no Herdr, installing through angi-update.

The swarm (orchestrator, builder, tester) is slow and, for a change to the desktop, more moving parts
than the job needs. An OS run is a single agent with its own model, the angi-gui skill, and the
guarded updater, in the terminal the user started it from.
"""
import json
import os
from pathlib import Path
import time

import graph_capture
import graph_recall
from models import active_selections
from scope import OS_SOURCE, scope_text
import session

AGENT = "angi-os"


def config(memory=""):
    """OpenCode configuration with the single OS agent and the model chosen for OS updates."""
    profile = {"description": "ANGI OS updates", "mode": "primary", "permission": {"*": "allow", "task": "deny"},
               "prompt": (session.ASSETS / "profiles/os.md").read_text() + "\n" + scope_text() + ("\n\n" + memory if memory else "")}
    model = active_selections().get("os")
    if model:
        profile["model"] = model
    return session.opencode_config({AGENT: profile})


def prepare(state, brief=""):
    if not (OS_SOURCE / "Cargo.toml").is_file():
        raise RuntimeError("Native source checkout is missing. Install the ANGI development SDK first.")
    session.install_skills()
    session.start_size_guard()
    os.environ["OPENCODE_CONFIG_CONTENT"] = config(graph_recall.prompt_text(state, "os", brief, shell=True))
    os.chdir(OS_SOURCE)


def launch(state, brief):
    prepare(state, brief)
    run = state / ("angi-os-" + time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}")
    run.mkdir(mode=0o700)
    (run / "run.json").write_text(json.dumps({"mode": "os", "solo": True, "brief": brief}, indent=2))
    graph_capture.run_started(state, run.name, "os", brief)
    from work_changes import begin
    begin(run, OS_SOURCE)
    marker = state / f"os-current.{os.getpid()}.new"
    marker.write_text(run.name)
    marker.replace(state / "os-current")
    os.environ.update(AGX_SWARM_RUN=str(run), AGX_SWARM_ROLE="os")
    os.execvp("opencode", ["opencode", "--agent", AGENT, "--prompt", brief])


def resume(state):
    prepare(state)
    name = (state / "os-current").read_text().strip()
    if Path(name).name == name and name.startswith("angi-os-") and (state / name / "run.json").is_file():
        os.environ.update(AGX_SWARM_RUN=str(state / name), AGX_SWARM_ROLE="os")
    os.execvp("opencode", ["opencode", "--agent", AGENT, "--continue"])


def newest(state):
    """True when the last run was an OS update run, not a swarm or a chat."""
    solo = state / "os-current"
    return solo.exists() and all(not other.exists() or solo.stat().st_mtime >= other.stat().st_mtime for other in (state / "current", state / "chat-current"))
