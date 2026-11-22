"""Chat: one OpenCode agent with optional tools for a selected local folder."""
import json
import os
from pathlib import Path
import subprocess
import time

from models import active_selections
from scope import chat_scope_text, validate_project
import graph_capture
import graph_recall
import session

AGENT = "angi-chat"
SKILL = "angi-chat"
MARKER = "chat-current"


def config(folder, memory=""):
    """Include Angi's persona and remembered context in the prompt so they are available on every provider."""
    skill = (session.ASSETS / "skills" / SKILL / "SKILL.md").read_text().split("---", 2)[2].strip()
    prompt = (session.ASSETS / "profiles/chat.md").read_text().strip()
    from chat_files import TOOLS
    permission = {"*": "deny", **{f"angi_files_{name}": "allow" for name in TOOLS}}
    profile = {"description": "Angi, in conversation", "mode": "primary", "permission": permission,
               "prompt": "\n\n".join(part for part in (prompt, chat_scope_text(folder), skill, memory) if part)}
    # Chat is a single agent like an OS update, so it shares that role's model; none chosen means OpenCode's default.
    model = active_selections().get("os")
    if model:
        profile["model"] = model
    result = json.loads(session.opencode_config({AGENT: profile}))
    result["mcp"] = {"angi_files": {
        "type": "local",
        "command": ["/usr/bin/python3", str(Path(__file__).with_name("chat_files.py").resolve()), str(folder)],
    }}
    return json.dumps(result)


def prepare(state, folder=None, message="", conversation=None):
    if folder is None:
        folder = str(Path.home().resolve())
    selected = validate_project(folder, allow_home=True)
    if str(selected) != folder:
        raise ValueError("The Chat folder has moved. Select it again.")
    session.start_size_guard()
    # Remembered context is chosen from what was just asked, before this exchange is itself remembered.
    memory = graph_recall.prompt_text(state, "chat", message, selected, ("conversation", conversation) if conversation else None)
    os.environ["OPENCODE_CONFIG_CONTENT"] = config(selected, memory)
    home = state / "chat"
    home.mkdir(mode=0o700, exist_ok=True)
    os.chdir(home)


def launch(state, brief):
    prepare(state, message=brief)
    run = state / ("angi-chat-" + time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}")
    run.mkdir(mode=0o700)
    (run / "run.json").write_text(json.dumps({"mode": "chat", "solo": True, "brief": brief}, indent=2))
    graph_capture.run_started(state, run.name, "chat", brief)
    marker = state / f"{MARKER}.{os.getpid()}.new"
    marker.write_text(run.name)
    marker.replace(state / MARKER)
    os.execvp("opencode", ["opencode", "--agent", AGENT, "--prompt", brief])


def resume(state):
    prepare(state)
    os.execvp("opencode", ["opencode", "--agent", AGENT, "--continue"])


def parse_reply(output):
    """Extract text, an error, and the session ID from OpenCode's JSON events."""
    text, error, session_id = [], None, None
    for line in output.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if not isinstance(event, dict):
            continue
        session_id = event.get("sessionID", session_id)
        if event.get("type") == "text":
            text.append(event.get("part", {}).get("text", ""))
        elif event.get("type") == "error":
            detail = event.get("error", {})
            error = (detail.get("data") or {}).get("message") or detail.get("message") or detail.get("name") or "Angi hit a problem."
    return "".join(text).strip(), error, session_id


def turn(state, message, new=False, timeout=300, folder=None, session_id=None):
    """One chat exchange for the desktop's chat view: {"text": reply} or {"error": reason}.

    The desktop continues a particular conversation by ID. The CLI also accepts the legacy continuation flag.
    """
    import history
    import sqlite3

    continuation = ["--session", session_id] if session_id else [] if new else ["--continue"]
    command = ["opencode", "run", "--agent", AGENT, "--format", "json", *continuation, "--", message]
    try:
        if session_id and not history.can_continue(state, session_id):
            return {"error": "This chat is no longer available."}
        prepare(state, folder, message, session_id)
    except (OSError, RuntimeError, ValueError, sqlite3.Error) as error:
        return {"error": str(error)}
    try:
        result = subprocess.run(command, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"error": "Angi took too long to answer. Try again."}
    except OSError:
        return {"error": "OpenCode is not available."}
    reply, error, returned_id = parse_reply(result.stdout)
    answer = {}
    if isinstance(returned_id, str) and history.CHAT_ID.fullmatch(returned_id):
        answer["session_id"] = returned_id
        history.remember_folder(state, returned_id, folder)
    if reply:
        graph_capture.chat_turn(state, answer.get("session_id") or session_id or "chat-" + time.strftime("%Y%m%d"), message, reply, folder)
        return {**answer, "text": reply}
    lines = result.stderr.strip().splitlines()
    return {**answer, "error": error or (lines[-1] if lines else "Angi did not answer. Check the model in Agent Angi's provider setup.")}


def newest(state):
    """True when the last run was a chat, not an OS update or a swarm."""
    chat = state / MARKER
    if not chat.exists():
        return False
    return all(not other.exists() or chat.stat().st_mtime >= other.stat().st_mtime for other in (state / "os-current", state / "current"))
