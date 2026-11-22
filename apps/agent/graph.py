"""ANGI's knowledge graph: what has been shared, changed and decided on this device.

It belongs to ANGI, not to an AI provider. Local models, OpenAI and Apple Intelligence all read the same
graph through the prompt, and none of them stores it. The file is private to the account
(`knowledge.sqlite3` beside the other Agent Angi state), and the desktop's Memory view can read, edit,
audit and clear every row.

Nodes are things worth remembering (a project, a system change, a conversation, a piece of work, a topic,
a note). Edges say how they relate. Events are the short timeline under a node: what was asked, what was
reported. Text is cleaned and stripped of secrets before it is stored.
"""
from contextlib import closing, contextmanager
import json
import os
from pathlib import Path
import re
import sqlite3
import time

KINDS = ("project", "system", "conversation", "work", "topic", "note", "person")
RELATIONS = ("about", "mentions", "changed", "related", "told")
NAME_LIMIT, SUMMARY_LIMIT, EVENT_LIMIT = 120, 600, 400
EVENTS_PER_NODE = 40
RECALL_LOG = 200
NODE_LIMIT = 5000
DATA_ROOT = Path("/var/lib/agx-data/agx-opencode-users")

SCHEMA = """
CREATE TABLE IF NOT EXISTS nodes(
    id INTEGER PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, name TEXT NOT NULL, summary TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT '', created REAL NOT NULL, updated REAL NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0, edited INTEGER NOT NULL DEFAULT 0, UNIQUE(kind, key));
CREATE TABLE IF NOT EXISTS edges(
    src INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE, dst INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    relation TEXT NOT NULL, created REAL NOT NULL, PRIMARY KEY(src, dst, relation));
CREATE TABLE IF NOT EXISTS events(
    id INTEGER PRIMARY KEY, node INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE, time REAL NOT NULL, text TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS forgotten(kind TEXT NOT NULL, key TEXT NOT NULL, PRIMARY KEY(kind, key));
CREATE TABLE IF NOT EXISTS recalls(id INTEGER PRIMARY KEY, time REAL NOT NULL, mode TEXT NOT NULL, query TEXT NOT NULL, names TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS edges_dst ON edges(dst);
CREATE INDEX IF NOT EXISTS events_node ON events(node);
"""

SECRET = re.compile(
    r"\b(?:sk|pk|rk)-[A-Za-z0-9_\-]{16,}"
    r"|\bgh[pousr]_[A-Za-z0-9]{20,}|\bgithub_pat_[A-Za-z0-9_]{20,}"
    r"|\bAKIA[0-9A-Z]{16}\b|\bxox[abprs]-[A-Za-z0-9-]{10,}"
    r"|\bBearer\s+[A-Za-z0-9._\-]{16,}"
    r"|\b(?:password|passwd|passphrase|secret|token|api[_ -]?key)\b\s*(?:is|:|=)\s*\S+"
    r"|\b[A-Fa-f0-9]{48,}\b", re.I)
PRIVATE_KEY = re.compile(r"-----BEGIN[ A-Z]*PRIVATE KEY-----.*", re.S)


def clean(text, limit):
    """One line of text with secrets hidden, cut at `limit` characters."""
    text = PRIVATE_KEY.sub("[hidden]", str(text))
    text = " ".join(SECRET.sub("[hidden]", text).split())
    return text if len(text) <= limit else text[:limit - 1].rstrip() + "…"


def state_dir(uid=None):
    """Agent Angi's per-account state folder; agents have XDG_STATE_HOME set, `angi-update` may not."""
    root = os.environ.get("XDG_STATE_HOME")
    if root and uid is None:
        return Path(root) / "angi-swarm"
    return DATA_ROOT / str(os.getuid() if uid is None else uid) / "state/angi-swarm"


class Graph:
    def __init__(self, connection, path=None):
        self.db = connection
        self.path = path

    def size(self):
        """Bytes the graph takes on disk (the database file, which holds every node, link and timeline)."""
        try:
            return self.path.stat().st_size if self.path else 0
        except OSError:
            return 0

    # -- switches ---------------------------------------------------------------------------------
    def enabled(self):
        row = self.db.execute("SELECT value FROM meta WHERE key='enabled'").fetchone()
        return row is None or row[0] != "off"

    def set_enabled(self, on):
        self.db.execute("INSERT OR REPLACE INTO meta VALUES('enabled', ?)", ("on" if on else "off",))

    # -- writing ----------------------------------------------------------------------------------
    def upsert(self, kind, key, name, summary=None, source="", pinned=False, replace=False):
        """The node's id, creating it if needed. Returns None for something the person chose to forget.

        A node the person edited keeps their wording: later captures add events and links, not new text.
        """
        if kind not in KINDS:
            raise ValueError("Unknown memory kind.")
        if self.db.execute("SELECT 1 FROM forgotten WHERE kind=? AND key=?", (kind, key)).fetchone():
            return None
        now = time.time()
        row = self.db.execute("SELECT id, edited FROM nodes WHERE kind=? AND key=?", (kind, key)).fetchone()
        if row is None:
            cursor = self.db.execute(
                "INSERT INTO nodes(kind, key, name, summary, source, created, updated, pinned) VALUES(?,?,?,?,?,?,?,?)",
                (kind, key, clean(name, NAME_LIMIT) or "Untitled", clean(summary or "", SUMMARY_LIMIT), source, now, now, int(pinned)))
            self.prune()
            return cursor.lastrowid
        if summary is not None and not row[1]:
            self.db.execute("UPDATE nodes SET summary=? WHERE id=?", (clean(summary, SUMMARY_LIMIT), row[0]))
            if replace:
                self.db.execute("UPDATE nodes SET name=? WHERE id=?", (clean(name, NAME_LIMIT) or "Untitled", row[0]))
        self.db.execute("UPDATE nodes SET updated=? WHERE id=?", (now, row[0]))
        return row[0]

    def event(self, node, text):
        text = clean(text, EVENT_LIMIT)
        if node is None or not text:
            return
        self.db.execute("INSERT INTO events(node, time, text) VALUES(?,?,?)", (node, time.time(), text))
        self.db.execute("DELETE FROM events WHERE node=? AND id NOT IN (SELECT id FROM events WHERE node=? ORDER BY id DESC LIMIT ?)",
                        (node, node, EVENTS_PER_NODE))

    def link(self, source, target, relation="related"):
        if source is None or target is None or source == target or relation not in RELATIONS:
            return
        self.db.execute("INSERT OR IGNORE INTO edges VALUES(?,?,?,?)", (source, target, relation, time.time()))

    def unlink(self, first, second):
        self.db.execute("DELETE FROM edges WHERE (src=? AND dst=?) OR (src=? AND dst=?)", (first, second, second, first))

    def edit(self, node, name, summary):
        name, summary = clean(name, NAME_LIMIT), clean(summary, SUMMARY_LIMIT)
        if not name:
            raise ValueError("Give this memory a name.")
        self.require(node)
        self.db.execute("UPDATE nodes SET name=?, summary=?, edited=1, updated=? WHERE id=?", (name, summary, time.time(), node))

    def pin(self, node, on):
        self.require(node)
        self.db.execute("UPDATE nodes SET pinned=? WHERE id=?", (int(on), node))

    def forget(self, node):
        """Delete a node with its links and timeline, and keep it from being captured again."""
        row = self.require(node)
        self.db.execute("INSERT OR IGNORE INTO forgotten VALUES(?,?)", (row["kind"], row["key"]))
        self.db.execute("DELETE FROM nodes WHERE id=?", (node,))

    def clear(self):
        """Forget everything, including what was forgotten one by one. The on/off choice stays."""
        for table in ("edges", "events", "nodes", "forgotten", "recalls"):
            self.db.execute(f"DELETE FROM {table}")

    def drop_siblings(self, kind, prefix, keep):
        """Remove superseded values of a one-value fact (an old name when a new one is given)."""
        self.db.execute("DELETE FROM nodes WHERE kind=? AND key LIKE ? AND id<>?", (kind, prefix + "%", keep))

    def get_meta(self, key):
        row = self.db.execute("SELECT value FROM meta WHERE key=?", (key,)).fetchone()
        return row[0] if row else None

    def set_meta(self, key, value):
        self.db.execute("INSERT OR REPLACE INTO meta VALUES(?,?)", (key, value))

    def prune(self):
        """Keep the graph bounded: the oldest unpinned conversations, work and topics go first."""
        extra = self.db.execute("SELECT COUNT(*) FROM nodes").fetchone()[0] - NODE_LIMIT
        if extra > 0:
            self.db.execute("DELETE FROM nodes WHERE id IN (SELECT id FROM nodes WHERE pinned=0 AND edited=0 AND kind IN ('conversation','work','topic') "
                            "ORDER BY updated LIMIT ?)", (extra,))

    def log_recall(self, mode, query, names):
        self.db.execute("INSERT INTO recalls(time, mode, query, names) VALUES(?,?,?,?)",
                        (time.time(), mode, clean(query, 160), json.dumps(names)))
        self.db.execute("DELETE FROM recalls WHERE id NOT IN (SELECT id FROM recalls ORDER BY id DESC LIMIT ?)", (RECALL_LOG,))

    # -- reading ----------------------------------------------------------------------------------
    def require(self, node):
        row = self.db.execute("SELECT * FROM nodes WHERE id=?", (node,)).fetchone()
        if row is None:
            raise ValueError("That memory is no longer there.")
        return row

    def find(self, kind, key):
        row = self.db.execute("SELECT id FROM nodes WHERE kind=? AND key=?", (kind, key)).fetchone()
        return row[0] if row else None

    def nodes(self, query="", kind="", limit=400):
        clauses, values = [], []
        if kind:
            clauses.append("n.kind=?")
            values.append(kind)
        for word in query.lower().split()[:6]:
            clauses.append("(lower(n.name) LIKE ? ESCAPE '\\' OR lower(n.summary) LIKE ? ESCAPE '\\')")
            pattern = "%" + word.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"
            values += [pattern, pattern]
        where = ("WHERE " + " AND ".join(clauses)) if clauses else ""
        return self.db.execute(
            f"SELECT n.*, (SELECT COUNT(*) FROM edges e WHERE e.src=n.id OR e.dst=n.id) AS links FROM nodes n {where} "
            "ORDER BY n.pinned DESC, n.updated DESC, n.id DESC LIMIT ?", (*values, limit)).fetchall()

    def counts(self):
        return {kind: count for kind, count in self.db.execute("SELECT kind, COUNT(*) FROM nodes GROUP BY kind")}

    def links(self, node):
        return self.db.execute(
            "SELECT n.id, n.kind, n.name, e.relation, CASE WHEN e.src=? THEN 'out' ELSE 'in' END AS direction FROM edges e "
            "JOIN nodes n ON n.id = CASE WHEN e.src=? THEN e.dst ELSE e.src END WHERE e.src=? OR e.dst=? "
            "ORDER BY n.kind, n.updated DESC", (node, node, node, node)).fetchall()

    def events(self, node):
        return self.db.execute("SELECT time, text FROM events WHERE node=? ORDER BY id DESC", (node,)).fetchall()

    def recalls(self, limit=8):
        return self.db.execute("SELECT time, mode, query, names FROM recalls ORDER BY id DESC LIMIT ?", (limit,)).fetchall()

    def export(self):
        rows = lambda sql: [dict(row) for row in self.db.execute(sql)]
        return {"format": "angi-knowledge-v1", "nodes": rows("SELECT * FROM nodes ORDER BY id"), "edges": rows("SELECT * FROM edges"),
                "events": rows("SELECT * FROM events ORDER BY id")}


@contextmanager
def open_graph(state=None, uid=None):
    """The graph, committed on success. A root caller (a workstation deploy) hands the file back to its owner."""
    folder = Path(state) if state else state_dir(uid)
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    path = folder / "knowledge.sqlite3"
    created = not path.exists()
    with closing(sqlite3.connect(path, timeout=10)) as connection:
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys=ON")
        connection.executescript(SCHEMA)
        try:
            yield Graph(connection, path)
            connection.commit()
        except BaseException:
            connection.rollback()
            raise
    if created:
        path.chmod(0o600)
        if os.getuid() == 0:
            owner = folder.stat()
            os.chown(path, owner.st_uid, owner.st_gid)
