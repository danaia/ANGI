"""Command dispatch for the native ANGI development swarm."""
import json
import os
from pathlib import Path
import sys
import subprocess

from messaging import inbox, send
import chat
import solo
from session import attach, launch, resume, status, dispatch
from ui import welcome
from models import activate, active, catalog, save, selections
from scope import describe, validate_project


def angi_request(args):
    """Return (brief, project, chat, single), or False for malformed arguments."""
    if args == ["angi"]:
        return None, None, False, False
    single = len(args) in (4, 6) and args[-1] == "--single"
    if single:
        args = args[:-1]
    if args[:2] == ["angi", "--prompt"] and len(args) in (3, 4, 5) and args[2].strip():
        if len(args) == 3:
            return args[2].strip(), None, False, single
        if len(args) == 4 and args[3] == "--chat":
            return args[2].strip(), None, True, False
        if len(args) == 5 and args[3] == "--project" and args[4].strip():
            return args[2].strip(), args[4], False, single
    return False


def main():
    # The private interpreter needs its own libraries at startup. Child tools
    # should use the live system libraries and their own wrapper environments.
    os.environ.pop("LD_LIBRARY_PATH", None)
    os.environ.pop("PYTHONHOME", None)
    args = sys.argv[1:]
    if args == ["provider-bridge"]:
        from providers import bridge
        bridge()
        return
    if args == ["models"]:
        print(json.dumps(selections()))
        return
    if len(args) == 2 and args[0] == "models":
        print(json.dumps(catalog(args[1])))
        return
    if args == ["provider"]:
        print(json.dumps(active()))
        return
    if len(args) == 2 and args[0] == "provider":
        print(json.dumps(activate(args[1])))
        return
    if len(args) == 3 and args[0] == "model":
        print(json.dumps(save(args[1], args[2])))
        return
    if len(args) == 2 and args[0] == "project":
        # The prompt asks this before it enables Start; launch checks again.
        try:
            print(json.dumps(describe(validate_project(args[1]))))
        except ValueError as error:
            print(json.dumps({"error": str(error)}))
        return
    if args == ["work-editor"]:
        from work_editor import serve
        serve(Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm")
        return
    if args[:1] == ["history"]:
        from history import request
        state = Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm"
        print(json.dumps(request(state, args[1:])))
        return
    if args[:1] == ["memory"]:
        # ANGI's knowledge graph, for the desktop's Memory view and for agents saving a note.
        from knowledge import request as memory
        print(json.dumps(memory(Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm", args[1:])))
        return
    if args[:1] == ["chat-turn"] and len(args) >= 2:
        # One exchange for the desktop's chat bubbles: a single JSON line, text or error.
        if os.getuid() == 0:
            raise RuntimeError("Run agent chat-turn as your desktop account.")
        options = args[1:-1]
        new = options[:1] == ["--new"]
        if new:
            options = options[1:]
        session_id = None
        if not new and len(options) >= 2 and options[0] == "--session":
            session_id = options[1]
            options = options[2:]
        if (options and not (len(options) == 2 and options[0] == "--folder")) or not args[-1].strip():
            raise ValueError("Usage: agent chat-turn [--new | --session ID] [--folder PATH] MESSAGE")
        state = Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm"
        state.mkdir(parents=True, exist_ok=True, mode=0o700)
        print(json.dumps(chat.turn(state, args[-1].strip(), new=new, folder=options[1] if options else None,
                                   session_id=session_id)))
        return
    if args and args[0] == "_dispatch":
        dispatch(Path(args[1]))
        return
    if args and args[0] == "tell" and len(args) >= 3:
        send(args[1], " ".join(args[2:]))
        return
    if args == ["inbox"]:
        inbox()
        return
    if args[:1] == ["report"] and len(args) >= 2:
        from work_history import save as save_report
        save_report(args[1], args[2:])
        print("Saved ANGI work summary.")
        return
    if len(args) == 2 and args[0] == "preview":
        from agent_preview import from_terminal
        print(from_terminal(args[1]))
        return
    if args == ["status"]:
        status()
        return
    if args == ["attach"]:
        if os.getuid() == 0:
            raise RuntimeError("Run agent attach as your desktop account.")
        attach(Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm")
        return
    request = angi_request(args)
    if request is False:
        print("Usage: agent angi [--prompt BRIEF [[--project DIR] [--single] | --chat]] | agent preview HTML_OR_LOCAL_ADDRESS | agent attach | agent status | agent tell ROLE MESSAGE | agent inbox | agent report SUMMARY [PATH ...] | agent memory [COMMAND]")
        return
    brief, project, talk, single = request
    if os.getuid() == 0:
        raise RuntimeError("Run agent angi as your desktop account. Use angi-root for privileged work.")
    state = Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm"
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    action, brief = ("start", brief) if brief else welcome((state / "current").exists() or (state / "os-current").exists() or (state / chat.MARKER).exists())
    if action == "connect":
        os.execvp("opencode", ["opencode", "/var/lib/agx-data/angi-dev"])
    elif action == "resume":
        # Resume the most recent Chat, single-agent or Herdr work session.
        (chat.resume if chat.newest(state) else solo.resume if solo.newest(state) else resume)(state)
    elif action == "start":
        if talk:
            chat.launch(state, brief)
        elif single:
            solo.launch(state, brief, project)
        else:
            launch(state, brief, project)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"ANGI: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)
