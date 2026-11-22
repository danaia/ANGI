"""Local MCP tools that let Chat keep ANGI's memory: remember, forget and search.

Like the memory tool in other agent runtimes, the model decides what is worth keeping; ANGI stores it in its
own knowledge graph, so it survives a change of AI provider. Forgetting is limited to what the user told ANGI
(facts and notes): chats, work and system history can only be removed by the person, in the Memory view.
"""
import json
import sys

import graph_capture
import graph_recall
from graph import clean, open_graph, state_dir

FORGET_LIMIT = 3


def remember(graph, text, about_user=True):
    text = clean(text, 600)
    if not text:
        raise ValueError("Say what to remember.")
    if not graph.enabled():
        raise ValueError("Memory is turned off. The user can turn it on in the Memory view.")
    kind = "person" if about_user else "note"
    node = graph.upsert(kind, "fact:" + text.lower(), text[:60], text, "angi")
    if node is None:
        return "The user asked ANGI to forget that, so it was not saved."
    if about_user:
        graph.link(node, graph_capture.hub(graph, graph_capture.YOU), "about")
    return f"Remembered: {text}"


def forget(graph, text):
    needle = clean(text, 200).lower()
    if len(needle) < 3:
        raise ValueError("Say what to forget.")
    found = [row for row in graph.nodes(limit=2000) if row["kind"] in ("person", "note") and not row["key"].startswith("hub:")
             and needle in (row["summary"] + " " + row["name"]).lower()]
    if not found:
        return "Nothing remembered matches that."
    if len(found) > FORGET_LIMIT:
        raise ValueError(f"{len(found)} memories match. Be more specific so only the right ones are forgotten.")
    for row in found:
        graph.forget(row["id"])
    return "Forgot: " + "; ".join(row["summary"] or row["name"] for row in found)


def search(graph, query):
    if not graph.enabled():
        return "Memory is turned off."
    graph_capture.backfill(graph)
    rows = graph_recall.rank(graph, query)[:5]
    user, notes = graph_recall.core(graph)
    wanted, lines = graph_recall.words(query), []
    import time
    now = time.time()
    for row in user + notes:
        if wanted & graph_recall.words(row["summary"] + " " + row["name"]):
            lines.append(f"- {row['summary'] or row['name']}")
    lines += [graph_recall.line_for(graph, row, wanted, now) for row in rows]
    return "\n".join(lines) if lines else "Nothing remembered matches that."


TEXT = {"type": "string"}
TOOLS = {
    "remember": (remember, "Save a lasting fact about the user or their preferences. Use it when they share something they would expect "
                           "you to know next time. Not for small talk.",
                 {"text": {**TEXT, "description": "One short sentence, such as 'The user's dog is named Rex.'"},
                  "about_user": {"type": "boolean", "description": "True for facts about the user; false for a note about the device or a project."}},
                 ["text"]),
    "forget": (forget, "Forget something the user told ANGI, when they ask you to.", {"text": {**TEXT, "description": "Words from the memory to forget."}}, ["text"]),
    "search": (search, "Search ANGI's memory of earlier chats, work and what the user has said.", {"query": TEXT}, ["query"]),
}


def respond(request):
    response = {"jsonrpc": "2.0", "id": request["id"]}
    method, params = request.get("method"), request.get("params", {})
    if method == "initialize":
        result = {"protocolVersion": "2025-03-26", "capabilities": {"tools": {}}, "serverInfo": {"name": "angi-memory", "version": "1.0"}}
    elif method == "ping":
        result = {}
    elif method == "tools/list":
        result = {"tools": [{"name": name, "description": description, "inputSchema": {
            "type": "object", "properties": properties, "required": required, "additionalProperties": False}}
            for name, (_, description, properties, required) in TOOLS.items()]}
    elif method == "tools/call":
        try:
            with open_graph(state_dir()) as graph:
                text, error = TOOLS[params["name"]][0](graph, **params.get("arguments", {})), False
        except (OSError, TypeError, ValueError, KeyError) as exc:
            text, error = (str(exc) if not isinstance(exc, OSError) else exc.strerror), True
        result = {"content": [{"type": "text", "text": text}], "isError": error}
    else:
        return {**response, "error": {"code": -32601, "message": "Method not found"}}
    return {**response, "result": result}


def serve():
    for line in sys.stdin:
        request = json.loads(line)
        if "id" in request:
            print(json.dumps(respond(request)), flush=True)


if __name__ == "__main__":
    serve()
