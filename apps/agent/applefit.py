"""Keep OpenCode's requests inside what an Apple Intelligence server accepts.

Two limits were measured on Local LLM Server (November 2026). The model's context is 8,192 tokens,
answered with a 400 `context_length_exceeded` that names the token count. An earlier instance of the
server also refused any body over 16 KB with 413 `{"error":true,"reason":"Payload Too Large"}`
(16,359 bytes passed, 16,384 did not); a restarted one accepted 64 KB. OpenCode's first request
alone is about 25 KB, roughly 8,000 tokens: nine tool definitions, the repository's instructions and
the list of skills, before any work has happened. Only this service is affected, so this module sits
between OpenCode and the Apple server and applies to it alone:

* `lean()` gives each agent fewer tools, which removes about a third of the request outright.
* `fit()` trims what is still too large, in an order that loses the least: long tool output, the
  skill list, tool descriptions, then the oldest conversation steps.
* `serve()` is a small loopback proxy that applies `fit()` to every request, so a long build log or a
  long session does not reach the server over its limit. Bytes only approximate tokens, so when the
  server still refuses a request, the proxy reads how far over it was, trims to that, retries (up to three times) and
  keeps the tighter budget. It also caps the reply length so prompt plus reply stay inside the context.
  The saved server address is read on every request, so connecting a different server needs no restart.
"""
import copy
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.request
from urllib.parse import urlsplit

PORT = 18081
URL = f"http://127.0.0.1:{PORT}/v1"
LIMIT = 15000          # bytes of request body to start from: under the old 16,384-byte cap, about 5,000 tokens
REPLY = 2048           # longest reply asked for; the context is shared by the prompt and the reply
FLOOR = 3000           # below this nothing useful fits, so a refusal is passed on instead of retried
OUTPUT_CAP = 4000      # characters kept of one tool result
IDLE = 3600            # seconds without a request before the proxy exits
# Tools that cost the most request space for the least use here: bash covers search and listing.
LEAN_DENY = ("webfetch", "todowrite", "glob", "grep", "skill")
NOTE = "[Earlier steps of this conversation were left out to fit the server's request size limit.]"
HOP_BY_HOP = {"connection", "keep-alive", "transfer-encoding", "content-length", "proxy-connection", "upgrade", "te", "trailer"}
STATS = {"requests": 0, "trimmed": 0, "largest_before": 0, "limit": LIMIT, "last": time.time()}


def dump(body):
    return json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode()


def clip(text, cap):
    if len(text) <= cap:
        return text
    keep = cap // 2
    return f"{text[:keep]}\n[… {len(text) - cap} characters left out to fit the server's request size limit …]\n{text[-keep:]}"


def clip_message(message, cap):
    content = message.get("content")
    if isinstance(content, str):
        message["content"] = clip(content, cap)
    elif isinstance(content, list):
        for part in content:
            if isinstance(part, dict) and isinstance(part.get("text"), str):
                part["text"] = clip(part["text"], cap)


def short_descriptions(node, cap):
    """The schema with every description text cut to `cap`; a property that is itself named "description" is left alone."""
    if isinstance(node, dict):
        return {key: value[:cap] if key == "description" and isinstance(value, str) else short_descriptions(value, cap) for key, value in node.items()}
    if isinstance(node, list):
        return [short_descriptions(item, cap) for item in node]
    return node


def clip_outputs(body, limit):
    for message in body["messages"]:
        if message.get("role") == "tool":
            clip_message(message, OUTPUT_CAP)


def drop_skill_list(body, limit):
    for message in body["messages"]:
        if message.get("role") == "system" and isinstance(message.get("content"), str):
            message["content"] = re.sub(r"\s*<available_skills>.*?</available_skills>", "", message["content"], flags=re.S)


def slim_tools(body, limit):
    for tool in body.get("tools") or []:
        function = tool.get("function", {})
        if isinstance(function.get("description"), str):
            function["description"] = function["description"][:300]
        if "parameters" in function:
            function["parameters"] = short_descriptions(function["parameters"], 60)


def steps(messages):
    """The conversation after the task itself, grouped so a tool call always travels with its results."""
    units = []
    for message in messages:
        if message.get("role") == "tool" and units and units[-1][0].get("tool_calls"):
            units[-1].append(message)
        else:
            units.append([message])
    return units


def drop_history(body, limit):
    messages = body["messages"]
    first = next((index for index, message in enumerate(messages) if message.get("role") == "user"), None)
    if first is None:
        return
    head, units = messages[:first + 1], steps(messages[first + 1:])
    dropped = False
    while len(units) > 1 and len(dump({**body, "messages": head + [m for unit in units for m in unit]})) > limit:
        units.pop(0)
        dropped = True
    if dropped and isinstance(head[-1].get("content"), str) and NOTE not in head[-1]["content"]:
        head[-1]["content"] += "\n\n" + NOTE
    body["messages"] = head + [m for unit in units for m in unit]


def clip_everything(body, limit):
    for message in body["messages"]:
        if message.get("role") != "system":
            clip_message(message, 1500)
    system = next((m for m in body["messages"] if m.get("role") == "system" and isinstance(m.get("content"), str)), None)
    excess = len(dump(body)) - limit
    if system and excess > 0:
        clip_message(system, max(1000, len(system["content"]) - excess - 200))


def fit(body, limit=LIMIT):
    """A copy of this chat request trimmed, if needed, to at most `limit` bytes. The original is untouched."""
    body = copy.deepcopy(body)
    for step in (clip_outputs, drop_skill_list, slim_tools, drop_history, clip_everything):
        if len(dump(body)) <= limit:
            break
        step(body, limit)
    return body


def trim(raw, limit=None):
    """The bytes to send for a request body: trimmed to the byte budget if too large, reply length capped.

    A body that is not a chat request is returned unchanged."""
    limit = limit or STATS["limit"]
    try:
        body = json.loads(raw)
        if not isinstance(body.get("messages"), list):
            return raw
        capped = isinstance(body.get("max_tokens"), int) and body["max_tokens"] > REPLY
        if capped:
            body["max_tokens"] = REPLY
        if len(raw) <= limit:
            return dump(body) if capped else raw
        trimmed = dump(fit(body, limit))
    except (ValueError, AttributeError, TypeError, KeyError):
        return raw
    STATS["trimmed"] += 1
    STATS["largest_before"] = max(STATS["largest_before"], len(raw))
    return trimmed


def tighten(sent, status, reply):
    """A smaller byte budget to retry with, from how the server refused `sent` bytes; None when there is nothing to learn."""
    if status == 413:
        budget = int(sent * 0.8)
    else:
        found = re.search(rb"maximum context length is (\d+) tokens.*?(?:resulted in|requested|have) (\d+) tokens", reply, re.S)
        if not found:
            return None
        window, used = int(found[1]), int(found[2])
        budget = int(sent * (window - REPLY) / max(used, 1) * 0.9)
    return budget if FLOOR <= budget < sent else None


def lean(profile):
    """An agent profile for a small-request server: the costliest rarely-needed tools off, skills read as files."""
    if profile.get("permission", {}).get("*") == "deny":
        return profile
    skills = Path(os.environ.get("XDG_CONFIG_HOME", "~/.config")).expanduser() / "opencode/skills"
    note = (f"\n\nThis model accepts only small requests. Keep tool output short (pipe long output through head or tail), "
            f"run one focused command at a time, and read a skill as a file when told to use it: {skills}/NAME/SKILL.md.")
    return {**profile, "permission": {**profile.get("permission", {}), **{name: "deny" for name in LEAN_DENY}}, "prompt": profile.get("prompt", "") + note}


def default_model(profiles, config):
    """The Apple model for an agent with none chosen: one already chosen for another role, else the server's first.

    Without this OpenCode falls back to the last model used in any service, so a builder with no Apple model
    quietly runs on another provider and fails there while the orchestrator waits for it."""
    chosen = next((p["model"] for p in profiles.values() if str(p.get("model", "")).startswith("apple/")), None)
    first = next(iter(config.get("models", {})), None)
    return chosen or (f"apple/{first}" if first else None)


def effective(choices, config, swarm_roles, all_roles):
    """The Apple model each role will run on, for display: what was chosen, else what a run would fall back to.

    Mirrors default_model(): swarm roles can inherit a swarm model, while standalone agents
    default to the server's first model."""
    first = next(iter(config.get("models", {})), None)
    first = f"apple/{first}" if first else None
    swarm = next((choices[role] for role in swarm_roles if str(choices.get(role, "")).startswith("apple/")), None)
    return {role: choices.get(role) or ((swarm or first) if role in swarm_roles else first) for role in all_roles if choices.get(role) or first}


def provider_config(config):
    """The saved Apple server configuration, pointed at the local size guard."""
    return {**config, "options": {**config.get("options", {}), "baseURL": URL}}


def upstream():
    path = Path(os.environ["XDG_CONFIG_HOME"]) / "opencode/angi-apple.json"
    return json.loads(path.read_text())["options"]["baseURL"].rstrip("/")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"  # the body ends when the connection closes, so streamed replies need no chunking

    def log_message(self, *_):
        pass

    def reply(self, status, payload):
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def relay(self, body):
        STATS["last"] = time.time()
        if self.path == "/applefit":
            return self.reply(200, {"applefit": True, **{key: value for key, value in STATS.items() if key != "last"}})
        STATS["requests"] += 1
        sent = trim(body) if self.command == "POST" else body
        try:
            connection, response = self.call(sent)
            for _ in range(3):
                if self.command != "POST" or response.status not in (400, 413):
                    break
                reply = response.read()
                connection.close()
                budget = tighten(len(sent), response.status, reply)
                retry = trim(body, budget) if budget else sent
                if retry == sent:
                    return self.pass_on(response.status, response.getheaders(), [reply])
                STATS["limit"], sent = budget, retry
                connection, response = self.call(sent)
        except (OSError, KeyError, ValueError) as error:
            return self.reply(502, {"error": {"message": f"The Apple Intelligence server could not be reached: {error}"}})
        try:
            self.pass_on(response.status, response.getheaders(), iter(lambda: response.read1(65536), b""))
        finally:
            connection.close()

    def call(self, body):
        target = urlsplit(upstream() + self.path.removeprefix("/v1"))
        connection = (http.client.HTTPSConnection if target.scheme == "https" else http.client.HTTPConnection)(target.hostname, target.port, timeout=900)
        headers = {name: value for name, value in self.headers.items() if name.lower() in ("content-type", "authorization", "accept")}
        connection.request(self.command, target.path + (f"?{target.query}" if target.query else ""), body or None, headers)
        return connection, connection.getresponse()

    def pass_on(self, status, headers, chunks):
        self.send_response(status)
        for name, value in headers:
            if name.lower() not in HOP_BY_HOP:
                self.send_header(name, value)
        self.end_headers()
        try:
            for chunk in chunks:
                self.wfile.write(chunk)
                self.wfile.flush()
        except OSError:
            pass

    def do_GET(self):
        self.relay(b"")

    def do_POST(self):
        self.relay(trim(self.rfile.read(int(self.headers.get("content-length") or 0))))


def serve():
    try:
        server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    except OSError:
        return  # another guard already owns the port
    server.daemon_threads = True
    server.timeout = 30
    while time.time() - STATS["last"] < IDLE:
        server.handle_request()


def alive():
    try:
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(f"http://127.0.0.1:{PORT}/applefit", timeout=1) as response:
            return bool(json.load(response).get("applefit"))
    except (OSError, ValueError):
        return False


def ensure():
    """Make sure the guard is running, starting it detached if it is not."""
    if alive():
        return
    subprocess.Popen([sys.executable, str(Path(__file__)), "serve"], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    for _ in range(50):
        if alive():
            return
        time.sleep(.1)
    raise RuntimeError("The Apple Intelligence request guard did not start.")


if __name__ == "__main__":
    os.environ.pop("LD_LIBRARY_PATH", None)
    os.environ.pop("PYTHONHOME", None)
    if sys.argv[1:] == ["serve"]:
        serve()
