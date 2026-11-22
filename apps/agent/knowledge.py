"""`agent memory ...`: the knowledge graph's command surface for the desktop's Memory view and for agents.

Every command prints one JSON object. Reads return the data; changes return {"done": true} (plus the new
node's id for a note) and the desktop re-reads, so this module keeps no view state.

    memory [list [QUERY] [KIND]]   overview: switch, counts, memories, and what agents were last told
    memory size                    bytes the graph takes on disk
    memory open ID                 one memory with its links and timeline
    memory edit ID NAME SUMMARY    rename or reword it (the wording then sticks)
    memory pin ID on|off           always include it for agents
    memory forget ID               delete it and keep it from being captured again
    memory unlink ID OTHER         remove the link between two memories
    memory note TEXT [ID]          save a note, optionally about another memory
    memory enabled on|off          turn remembering on or off
    memory clear                   forget everything
    memory export                  everything, as JSON
"""
import json
import sqlite3
import time

import graph_capture
from graph import KINDS, open_graph
from graph_recall import age


def when(then, now):
    return age(then, now)


def summary(row, now):
    return {"id": row["id"], "kind": row["kind"], "name": row["name"], "summary": row["summary"], "when": when(row["updated"], now),
            "pinned": bool(row["pinned"]), "edited": bool(row["edited"]), "source": row["source"], "links": row["links"]}


def overview(graph, query="", kind=""):
    if kind and kind not in KINDS:
        raise ValueError("Unknown memory type.")
    now = time.time()
    rows = graph.nodes(query, kind)
    used = []
    for row in graph.recalls():
        names = json.loads(row["names"])
        used.append({"when": when(row["time"], now), "mode": row["mode"], "names": ", ".join(names) if names else "nothing"})
    return {"enabled": graph.enabled(), "bytes": graph.size(), "counts": graph.counts(), "nodes": [summary(row, now) for row in rows], "used": used}


def detail(graph, node):
    row = graph.require(node)
    now = time.time()
    count = graph.db.execute("SELECT COUNT(*) FROM edges WHERE src=? OR dst=?", (node, node)).fetchone()[0]
    return {**summary({**dict(row), "links": count}, now), "created": time.strftime("%Y-%m-%d %H:%M", time.localtime(row["created"])),
            "events": [{"when": time.strftime("%b %d %H:%M", time.localtime(event["time"])), "text": event["text"]} for event in graph.events(node)],
            "links": [{"id": link["id"], "kind": link["kind"], "name": link["name"], "relation": link["relation"], "direction": link["direction"]}
                      for link in graph.links(node)]}


def number(text):
    try:
        return int(text)
    except ValueError:
        raise ValueError("Choose a memory first.") from None


def switch(text):
    if text not in ("on", "off"):
        raise ValueError("Use on or off.")
    return text == "on"


def run(state, args):
    command, rest = (args[0], list(args[1:])) if args else ("list", [])
    if command == "note" and 1 <= len(rest) <= 2:
        about = number(rest[1]) if len(rest) == 2 else None
        return {"done": True, "id": graph_capture.note(rest[0], about, "you", state)}
    with open_graph(state) as graph:
        if command == "list" and len(rest) <= 2:
            return overview(graph, *rest)
        if command == "size" and not rest:
            return {"bytes": graph.size()}
        if command == "open" and len(rest) == 1:
            return {"node": detail(graph, number(rest[0]))}
        if command == "edit" and len(rest) == 3:
            graph.edit(number(rest[0]), rest[1], rest[2])
        elif command == "pin" and len(rest) == 2:
            graph.pin(number(rest[0]), switch(rest[1]))
        elif command == "forget" and len(rest) == 1:
            graph.forget(number(rest[0]))
        elif command == "unlink" and len(rest) == 2:
            graph.unlink(number(rest[0]), number(rest[1]))
        elif command == "enabled" and len(rest) == 1:
            graph.set_enabled(switch(rest[0]))
        elif command == "clear" and not rest:
            graph.clear()
        elif command == "export" and not rest:
            return graph.export()
        else:
            raise ValueError("Use agent memory [list | size | open ID | edit ID NAME SUMMARY | pin ID on|off | forget ID | unlink ID OTHER | note TEXT [ID] | enabled on|off | clear | export].")
        return {"done": True}


def request(state, args=()):
    try:
        return run(state, list(args))
    except ValueError as error:
        return {"error": str(error)}
    except (OSError, sqlite3.Error):
        return {"error": "Memory could not be read or saved. Try again."}
