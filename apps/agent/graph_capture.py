"""Turn what happens on this device into knowledge-graph nodes, with no model involved.

Capture is deterministic so it behaves the same on every AI provider and costs no tokens: chats and
runs become nodes, the folders and ANGI features they mention become links, and OS releases and app
installs become system nodes. Every entry point is wrapped so that remembering can never break a chat,
a build or an update.
"""
import functools
import os
from pathlib import Path
import re
import time

from graph import clean, open_graph

# Areas of ANGI worth linking on sight. Anything else a person names in `backticks` becomes a topic too.
VOCABULARY = (
    "wi-fi", "wifi", "bluetooth", "tailscale", "lm studio", "apple intelligence", "openai", "opencode", "herdr",
    "terminal", "browser", "calendar", "night light", "theme", "dark mode", "kernel", "desktop", "files", "launcher",
    "settings", "network", "vnc", "storage", "nvme", "update", "release", "design system", "pebble line", "wayland",
)
BACKTICKED = re.compile(r"`([^`\n]{2,40})`")
TOPIC_LIMIT = 6
OS_HUB = ("system", "hub:os", "ANGI operating system", "Operating system updates, releases and changes made on this device.")
APPS_HUB = ("system", "hub:apps", "Apps", "Command-line tools and servers installed from the ANGI app library.")


def safe(function):
    """Run a capture, hiding any failure: memory is a convenience and must never interrupt the work."""
    @functools.wraps(function)
    def guarded(*args, **kwargs):
        try:
            return function(*args, **kwargs)
        except Exception:  # noqa: BLE001 - see above
            return None
    return guarded


def topics_in(text):
    """Feature names and `backticked` terms in the text, in order, without duplicates."""
    lowered = text.lower()
    found = [term for term in VOCABULARY if re.search(r"(?<![\w-])" + re.escape(term) + r"(?![\w-])", lowered)]
    found += [clean(term, 40) for term in BACKTICKED.findall(text)]
    unique = dict.fromkeys(term.strip().lower() for term in found if term.strip())
    return [term for term in unique][:TOPIC_LIMIT]


YOU = ("person", "hub:you", "You", "What you have told ANGI about yourself.")
SENTENCE = re.compile(r"(?<=[.!?])\s+|\n+")
STOP_VALUE = r"(?=\s+(?:and|but|so|because|since|though|while|where|which|who)\b|[,.;!?]|$)"
LEAD = r"^(?:(?:hi|hey|hello|ok|okay|so|well|btw|also)(?: angi)?[,! ]+)*"
RELATIVES = "dog|cat|pet|wife|husband|partner|girlfriend|boyfriend|son|daughter|kid|child|brother|sister|mom|mother|dad|father|birthday|job|profession|occupation|timezone|time zone|pronouns|age|hobby|hobbies|editor|laptop|phone|email|favorite \\w+|favourite \\w+"
# (slot, label, pattern, sentence). A slot holds one value: a new one replaces the old.
SLOTS = (
    ("name", "Name", re.compile(LEAD + r".*?\b(?:my name is|my name's|call me|i go by|people call me|i am called|i'm called)\s+([a-z][\w'’.-]*(?:\s+[a-z][\w'’.-]*){0,3}?)" + STOP_VALUE, re.I),
     "The user's name is {}."),
    ("location", "Location", re.compile(r"\bi\s*(?:'m|am)?\s*(?:live|living|reside|stay|based|located)\s+(?:in|at|near|around)\s+(.+?)" + STOP_VALUE, re.I), "The user lives in {}."),
    ("location", "Location", re.compile(r"\bi(?:'m| am)\s+from\s+(.+?)" + STOP_VALUE, re.I), "The user is from {}."),
    ("work", "Work", re.compile(r"\bi\s*(?:'m|am)?\s*(?:work|working)\s+((?:at|for|as|in|on)\s+.+?)" + STOP_VALUE, re.I), "The user works {}."),
)
RELATIVE = re.compile(r"\bmy\s+(" + RELATIVES + r")\s+(?:is|are|was)\s+(.+?)" + STOP_VALUE, re.I)
LIKES = re.compile(r"\bi\s+(?:really\s+|also\s+)?(prefer|like|love|enjoy|hate|dislike)\s+(.{3,100}?)" + STOP_VALUE, re.I)
REMEMBER = re.compile(r"\b(?:please\s+)?(?:remember|don'?t forget|keep in mind|note)\s+(?:that\s+)?(.{4,300})", re.I)
FACT_LIMIT = 6


def tidy(value, slot):
    value = value.strip(" .,;:!?\"'")
    return value.title() if slot in ("name", "location") and value.islower() else value


def user_facts(text):
    """What the person says about themselves, as (slot, key, label, sentence): deterministic, no model."""
    facts = []
    for sentence in SENTENCE.split(clean(text, 4000).replace("\\n", " ")):
        sentence = sentence.strip()
        if not sentence or sentence.endswith("?"):
            continue
        for slot, label, pattern, template in SLOTS:
            found = pattern.search(sentence)
            if found and (value := tidy(found.group(1), slot)):
                facts.append((slot, f"{slot}:{value.lower()}", label, template.format(value)))
        if found := RELATIVE.search(sentence):
            noun, value = found.group(1).lower(), tidy(found.group(2), "")
            facts.append((f"my-{noun}", f"my-{noun}:{value.lower()}", noun.capitalize(), f"The user's {noun} is {value}."))
        if found := LIKES.search(sentence):
            verb, value = found.group(1).lower(), tidy(found.group(2), "")
            facts.append(("", f"likes:{verb} {value.lower()}", f"{verb.capitalize()}s {value}"[:60], f"The user {verb}s {value}."))
        if found := REMEMBER.search(sentence):
            value = tidy(found.group(1), "")
            # "Remember that my cat is Tux" is the specific fact, not also a generic one.
            facts += user_facts(value) or [("", f"fact:{value.lower()}", value[:60], value[0].upper() + value[1:] + ("" if value[-1] in ".!" else "."))]
    return list(dict.fromkeys(facts))[:FACT_LIMIT]


def remember_facts(graph, text, conversation=None):
    """Save what the person said about themselves under the You node, linked to where they said it."""
    backfill(graph)   # an older graph catches up first, so it never replays over newer facts
    saved = []
    for slot, key, label, sentence in user_facts(text):
        # "Remember that ..." is about the person only when it says so; otherwise it is a note about the device or work.
        about_you = not key.startswith("fact:") or re.match(r"(?:my|i|i'm|i've|we|our)\b", key[5:])
        kind = "person" if about_you else "note"
        node = graph.upsert(kind, key, label, sentence, "told", replace=True)
        if node is None:
            continue
        if slot:
            graph.drop_siblings(kind, slot + ":", node)
        if about_you:
            graph.link(node, hub(graph, YOU), "about")
        graph.link(conversation, node, "told" if about_you else "related")
        saved.append(node)
    return saved


def backfill(graph):
    """Once per graph: find facts in conversations captured before ANGI learned to look for them."""
    if graph.get_meta("facts-v1") or not graph.enabled():
        return
    graph.set_meta("facts-v1", "done")
    rows = graph.db.execute("SELECT e.node, e.text FROM events e JOIN nodes n ON n.id=e.node WHERE n.kind IN ('conversation','work') "
                            "AND e.text LIKE 'You: %' ORDER BY e.id").fetchall()
    for node, text in rows:
        remember_facts(graph, text[5:], node)


def folder_node(graph, folder):
    """A project node for a real folder, never for the home folder itself."""
    if not folder:
        return None
    path = Path(folder)
    if path == Path.home() or not path.name:
        return None
    return graph.upsert("project", str(path), path.name, f"Folder at {path}", "capture")


def hub(graph, spec):
    kind, key, name, summary = spec
    return graph.upsert(kind, key, name, summary, "capture")


def mention(graph, node, text):
    for term in topics_in(text):
        graph.link(node, graph.upsert("topic", term, term, source="capture"), "mentions")


@safe
def chat_turn(state, conversation, message, reply, folder=None):
    """One exchange: the conversation node, its timeline, the folder it used and the topics it touched."""
    with open_graph(state) as graph:
        if not graph.enabled():
            return
        node = graph.upsert("conversation", conversation, message, "", "chat")
        if node is None:
            return
        row = graph.require(node)
        graph.event(node, "You: " + message)
        if reply:
            graph.event(node, "Angi: " + reply)
        if not row["edited"]:
            graph.db.execute("UPDATE nodes SET summary=? WHERE id=?", (clean("Latest ask: " + message, 600), node))
        graph.link(node, folder_node(graph, folder), "about")
        remember_facts(graph, message, node)
        mention(graph, node, message + " " + (reply or ""))


@safe
def run_started(state, run, mode, brief, project=None):
    """An OS update, project build or terminal chat that has begun."""
    with open_graph(state) as graph:
        if not graph.enabled():
            return
        label = {"os": "OS update", "project": "Project work", "chat": "Terminal chat"}.get(mode, "Work")
        kind = "conversation" if mode == "chat" else "work"
        node = graph.upsert(kind, run, brief, f"{label} requested: {brief}", "run")
        if node is None:
            return
        graph.event(node, f"{label} requested: {brief}")
        if mode == "os":
            graph.link(node, hub(graph, OS_HUB), "about")
        graph.link(node, folder_node(graph, project), "about")
        remember_facts(graph, brief, node)
        mention(graph, node, brief)


@safe
def run_report(state, run, summary, folders=()):
    """What an OS or project run says it did, kept on the run's node."""
    with open_graph(state) as graph:
        node = graph.find("work", run)
        if node is None or not graph.enabled():
            return
        row = graph.require(node)
        graph.event(node, "Report: " + summary)
        if not row["edited"]:
            graph.db.execute("UPDATE nodes SET summary=? WHERE id=?", (clean("Done: " + summary, 600), node))
        for folder in folders:
            graph.link(node, folder_node(graph, folder), "about")
        mention(graph, node, summary)


def current_work(graph):
    """The work node of the OS run this process belongs to, if it is inside one."""
    run = os.environ.get("AGX_SWARM_RUN")
    return graph.find("work", Path(run).name) if run else None


@safe
def release(release_name, origin, previous=None, uid=None):
    """A desktop release that was installed (or synced from the workstation)."""
    digest = release_name.rpartition(".")[2][:8] or release_name
    with open_graph(None, uid) as graph:
        if not graph.enabled():
            return
        when = time.strftime("%Y-%m-%d %H:%M")
        replaced = f", replacing {previous.rpartition('.')[2][:8]}" if previous else ""
        node = graph.upsert("system", "release:" + release_name, f"Desktop release {digest}",
                            f"Installed {when} from the {origin} build{replaced}.", "update")
        if node is None:
            return
        graph.event(node, f"Installed from the {origin} build{replaced}.")
        graph.link(node, hub(graph, OS_HUB), "about")
        graph.link(current_work(graph), node, "changed")


@safe
def app_change(app_id, name, action, version=""):
    """An app from the library was installed, updated, repaired or removed."""
    with open_graph() as graph:
        if not graph.enabled():
            return
        label = f"{name} {version}".strip()
        node = graph.upsert("system", "app:" + app_id, name, f"{action}: {label}.", "update")
        if node is None:
            return
        graph.event(node, f"{action}: {label}.")
        graph.link(node, hub(graph, APPS_HUB), "about")
        graph.link(current_work(graph), node, "changed")


def note(text, about=None, source="agent", state=None):
    """Something durable worth keeping (a decision, preference or fact), written by the person or an agent.

    Unlike automatic capture this reports problems, because someone asked for it."""
    with open_graph(state) as graph:
        if not graph.enabled():
            raise ValueError("Memory is turned off. Turn it on in the Memory view to save notes.")
        text = clean(text, 600)
        if not text:
            raise ValueError("Write what to remember.")
        if about is not None:
            graph.require(about)
        node = graph.upsert("note", f"note:{time.time_ns()}", text[:80], text, source)
        if about is not None:
            graph.link(node, about, "related")
        else:
            graph.link(node, current_work(graph), "related")
        return node
