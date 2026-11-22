"""Single OpenCode runs for OS and Project, including resume for older OS runs."""
import json
import os
from pathlib import Path
import time

import graph_capture
import graph_recall
from models import SINGLE_ROLE, active_selections
from scope import OS_SOURCE, PREVIEW_HINT, scope_text, validate_project
import session

AGENT = "angi-single"
LEGACY_AGENT = "angi-os"
MARKER = "os-current"


def config(project, memory, *, legacy=False):
    agent, role = (LEGACY_AGENT, "os") if legacy else (AGENT, SINGLE_ROLE)
    instructions = "os.md" if legacy else "single.md"
    profile = {"description": "ANGI single agent", "mode": "primary", "permission": {"*": "allow", "task": "deny"},
               "prompt": (session.ASSETS / "profiles" / instructions).read_text() + "\n" + scope_text(project)
               + "\n\n" + PREVIEW_HINT + ("\n\n" + memory if memory else "")}
    model = active_selections().get(role)
    if model:
        profile["model"] = model
    return session.opencode_config({agent: profile})


def prepare(state, project, brief="", *, legacy=False):
    workdir = project if project is not None else OS_SOURCE
    if project is None and not (workdir / "Cargo.toml").is_file():
        raise RuntimeError("Native source checkout is missing. Install the ANGI development SDK first.")
    session.install_skills()
    session.start_size_guard()
    memory = graph_recall.prompt_text(state, "os" if project is None else "project", brief, project, shell=True)
    os.environ["OPENCODE_CONFIG_CONTENT"] = config(project, memory, legacy=legacy)
    os.chdir(workdir)
    return workdir


def launch(state, brief, project=None):
    project = validate_project(project) if project is not None else None
    workdir = prepare(state, project, brief)
    run = state / ("angi-single-" + time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}")
    run.mkdir(mode=0o700)
    mode = "os" if project is None else "project"
    folder = str(project) if project is not None else None
    (run / "run.json").write_text(json.dumps({"mode": mode, "project": folder, "solo": True, "brief": brief}, indent=2))
    graph_capture.run_started(state, run.name, mode, brief, folder)
    from work_changes import begin
    begin(run, workdir)
    marker = state / f"{MARKER}.{os.getpid()}.new"
    marker.write_text(run.name)
    marker.replace(state / MARKER)
    os.environ.update(AGX_SWARM_RUN=str(run), AGX_SWARM_ROLE=SINGLE_ROLE)
    os.execvp("opencode", ["opencode", "--agent", AGENT, "--prompt", brief])


def resume(state):
    from history import build_row, database
    from work_history import matching_session
    name = (state / MARKER).read_text().strip()
    run = state / name
    if Path(name).name != name or not name.startswith(("angi-os-", "angi-single-")):
        raise ValueError("Invalid saved single-agent run.")
    if name.startswith("angi-os-"):
        prepare(state, None, legacy=True)
        if (run / "run.json").is_file():
            os.environ.update(AGX_SWARM_RUN=str(run), AGX_SWARM_ROLE="os")
        os.execvp("opencode", ["opencode", "--agent", LEGACY_AGENT, "--continue"])
        return
    row = build_row(run / "run.json")
    if row is None:
        raise ValueError("The saved single-agent run is missing.")
    project = validate_project(row["folder"]) if row["mode"] == "project" else None
    workdir = project if project is not None else OS_SOURCE
    with database() as db:
        identifier = matching_session(db, str(workdir), row["brief"], row["updated"], agent=AGENT)
    if identifier is None:
        raise ValueError("No OpenCode conversation was found for this single-agent run.")
    prepare(state, project)
    os.environ.update(AGX_SWARM_RUN=str(run), AGX_SWARM_ROLE=SINGLE_ROLE)
    os.execvp("opencode", ["opencode", "--agent", AGENT, "--session", identifier])


def newest(state):
    """True when the single-agent run is the most recent."""
    solo = state / MARKER
    return solo.exists() and all(not other.exists() or solo.stat().st_mtime >= other.stat().st_mtime for other in (state / "current", state / "chat-current"))
