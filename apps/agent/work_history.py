"""Evidence and folder links for an OS or Project run."""
import json
import os
from pathlib import Path
import sqlite3

from messaging import current_run
from models import atomic_write
from scope import OS_SOURCE


def folder(path):
    try:
        path = Path(path).expanduser().resolve(strict=True)
        return path if path.is_dir() else path.parent if path.is_file() else None
    except (OSError, RuntimeError, ValueError):
        return None


def save(summary, paths):
    run = current_run()
    if os.environ.get("AGX_SWARM_ROLE") not in ("os", "orchestrator") or not (run / "run.json").is_file():
        raise ValueError("Save the work summary from the OS agent or project orchestrator.")
    summary = summary.strip()
    if not summary or len(summary) > 12000:
        raise ValueError("Enter a work summary between 1 and 12000 characters.")
    folders = list(dict.fromkeys(str(path) for value in paths if (path := folder(value))))[:12]
    atomic_write(run / "report.json", {"summary": summary, "folders": folders})
    import graph_capture
    graph_capture.run_report(run.parent, run.name, summary, folders)
    from work_changes import finish
    try:
        finish(run)
    except OSError as error:
        print(f"ANGI could not capture the changed files: {error}", flush=True)


def matching_session(db, directory, brief, started):
    """Match legacy runs by workspace, exact first prompt and start time."""
    if db is None or not directory:
        return None
    rows = db.execute("""
        SELECT s.id
        FROM session s
        WHERE s.parent_id IS NULL AND s.directory=? AND s.time_created BETWEEN ? AND ?
          AND trim((SELECT json_extract(p.data,'$.text')
                    FROM message m JOIN part p ON p.session_id=m.session_id AND p.message_id=m.id
                    WHERE m.session_id=s.id AND json_extract(m.data,'$.role')='user'
                      AND json_extract(p.data,'$.type')='text'
                      AND COALESCE(json_extract(p.data,'$.synthetic'),0)=0
                      AND COALESCE(json_extract(p.data,'$.ignored'),0)=0
                    ORDER BY m.time_created,m.id,p.id LIMIT 1)) IN (?,?)
        LIMIT 2
    """, (directory, started - 10000, started + 180000, brief.strip(), "The user wants: " + brief.strip())).fetchall()
    return rows[0][0] if len(rows) == 1 else None


def recorded_reply(db, directory, brief, started):
    session = matching_session(db, directory, brief, started)
    if session is None:
        return ""
    parts = db.execute("""
        SELECT json_extract(p.data,'$.text') FROM part p
        WHERE p.session_id=? AND json_extract(p.data,'$.type')='text'
          AND COALESCE(json_extract(p.data,'$.synthetic'),0)=0
          AND COALESCE(json_extract(p.data,'$.ignored'),0)=0
          AND p.message_id=(SELECT m.id FROM message m
              WHERE m.session_id=p.session_id AND json_extract(m.data,'$.role')='assistant'
                AND json_extract(m.data,'$.finish')='stop'
                AND EXISTS (SELECT 1 FROM part t WHERE t.session_id=m.session_id AND t.message_id=m.id
                  AND json_extract(t.data,'$.type')='text' AND trim(json_extract(t.data,'$.text'))!=''
                  AND COALESCE(json_extract(t.data,'$.synthetic'),0)=0
                  AND COALESCE(json_extract(t.data,'$.ignored'),0)=0)
              ORDER BY m.time_created DESC,m.id DESC LIMIT 1)
        ORDER BY p.id
    """, (session,))
    text = "".join(value for value, in parts if isinstance(value, str)).strip()
    return text if len(text) <= 12000 else text[:11999].rstrip() + "…"


def details(run, row, db):
    try:
        report = json.loads((run / "report.json").read_text())
    except (OSError, ValueError):
        report = {}
    if not isinstance(report, dict):
        report = {}
    directory = str(OS_SOURCE) if row["mode"] == "os" else row["folder"]
    links = [("ANGI source folder" if row["mode"] == "os" else "Project folder", directory),
             ("Run records", str(run))]
    extra = report.get("folders")
    if isinstance(extra, list):
        links.extend(("Related folder", value) for value in extra[:12] if isinstance(value, str))
    resources = {}
    for label, value in links:
        if value and (path := folder(value)):
            resources.setdefault(str(path), {"label": label, "path": str(path)})
    summary = report.get("summary")
    if isinstance(summary, str) and summary.strip():
        label = "Work summary"
    else:
        try:
            summary = recorded_reply(db, directory, row["brief"], row["updated"])
        except sqlite3.Error:
            summary = ""
        label = "Last recorded agent reply" if summary else ""
    return {"summary": summary or "", "summary_label": label, "resources": list(resources.values())}


def swarm_peers(db, session, root):
    """Herdr opens separate top-level OpenCode sessions for the builder and tester."""
    try:
        rows = db.execute("""
            SELECT peer.id
            FROM session AS leader JOIN session AS peer ON peer.directory=leader.directory
            WHERE leader.id=? AND leader.directory=? AND leader.agent='angi-orchestrator'
                AND peer.parent_id IS NULL AND peer.agent IN ('angi-builder', 'angi-tester')
                AND peer.time_created BETWEEN leader.time_created AND leader.time_created+60000
                AND NOT EXISTS (
                    SELECT 1 FROM session AS next
                    WHERE next.directory=leader.directory AND next.agent='angi-orchestrator'
                        AND next.parent_id IS NULL AND next.time_created > leader.time_created
                        AND next.time_created <= peer.time_created
                )
            GROUP BY peer.agent HAVING count(*)=1
        """, (session, str(root)))
        return [row[0] for row in rows]
    except sqlite3.Error:
        return []


def legacy_changes(db, root, row):
    """Recover explicitly recorded file operations from an unambiguous legacy session."""
    from work_changes import EVIDENCE_LIMIT, FILE_LIMIT, parts

    try:
        session = matching_session(db, str(root), row["brief"], row["updated"])
        if session is None:
            return None
        roots = [session]
        if row["mode"] == "project":
            roots.extend(swarm_peers(db, session, root))
        rows = db.execute("""
            WITH RECURSIVE sessions(id) AS (
                SELECT value FROM json_each(?)
                UNION SELECT s.id FROM session s JOIN sessions p ON s.parent_id=p.id WHERE s.directory=?
            )
            SELECT json_extract(p.data,'$.tool'), json_extract(p.data,'$.state.metadata'),
                   json_extract(p.data,'$.state.input.content')
            FROM part p JOIN sessions s ON p.session_id=s.id
            WHERE json_extract(p.data,'$.type')='tool' AND json_extract(p.data,'$.state.status')='completed'
                AND json_extract(p.data,'$.tool') IN ('edit','write','apply_patch')
            ORDER BY p.time_created,p.id
            LIMIT 1000
        """, (json.dumps(roots), str(root))).fetchall()
    except sqlite3.Error:
        return None
    changes, total = {}, 0
    for tool, payload, text in rows:
        meta = json.loads(payload or "{}")
        if not isinstance(meta, dict):
            continue
        if tool == "apply_patch":
            edits = [(item.get("filePath"), item.get("type"), item.get("patch"), None)
                     for item in (meta.get("files") or []) if isinstance(item, dict)]
        elif tool == "edit":
            diff = meta.get("filediff")
            edits = [(diff.get("file"), "modify", diff.get("patch"), None)] if isinstance(diff, dict) else []
        else:
            added = meta.get("exists") is False
            edits = [(meta.get("filepath"), "add" if added else "modify", None, text if added else None)]
        for path, kind, patch, content in edits:
            if not isinstance(path, str):
                continue
            path = Path(path)
            try:
                if path.is_absolute():
                    path = path.relative_to(root)
                relative = path.as_posix()
                parts(relative)
            except ValueError:
                continue
            item = changes.setdefault(relative, {"kind": {"add": "Added", "delete": "Deleted"}.get(kind, "Modified"),
                                                 "original": None, "recorded": None, "patch": ""})
            if kind == "delete":
                item["kind"] = "Deleted"
            if isinstance(content, str) and len(content.encode()) <= FILE_LIMIT and total + len(content.encode()) <= EVIDENCE_LIMIT:
                item.update(original="", recorded=content)
                total += len(content.encode())
            elif isinstance(patch, str):
                size = len(patch.encode())
                if size <= FILE_LIMIT and total + size <= EVIDENCE_LIMIT:
                    item["patch"] += patch + "\n"
                    total += size
                item.update(original=None, recorded=None)
    return {"files": changes, "limited": False, "legacy": True} if changes else None
