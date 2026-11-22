"""Open an agent's HTML result or running local app in ANGI Web."""
import json
import os
from pathlib import Path
import subprocess

from models import atomic_write


HTML = (".html", ".htm", ".xhtml")
ENTRY_POINTS = ("dist/index.html", "build/index.html", "index.html", "index.htm", "index.xhtml")


def is_html(path):
    return Path(path).suffix.lower() in HTML


def preferred_page(pages):
    return min(pages, key=lambda path: (path.stem.lower() != "index", len(path.parts), path.as_posix()))


def checked(path, scope):
    """Resolve a visible file below the allowed folder, excluding symlink traversal."""
    path = Path(path)
    if scope is None:
        return path.resolve(strict=True)
    root = Path(scope)
    relative = path.relative_to(root)
    current = root
    for part in relative.parts:
        current /= part
        if part.startswith(".") or current.is_symlink():
            raise ValueError("Choose a visible HTML file inside the Chat workspace.")
    return current


def entry_point(target, scope=None):
    candidate = checked(target, scope)
    if candidate.is_file() and is_html(candidate):
        return candidate
    if candidate.is_dir():
        for name in ENTRY_POINTS:
            try:
                page = checked(candidate / name, candidate)
            except ValueError:
                continue
            if page.is_file():
                return page
        pages = [checked(page, scope) for page in candidate.iterdir()
                 if not page.name.startswith(".") and not page.is_symlink() and page.is_file() and is_html(page)]
        if len(pages) == 1:
            return pages[0]
    raise ValueError("Choose an HTML file or a folder with an HTML entry page.")


def state_path():
    return Path(os.environ["XDG_STATE_HOME"]) / "angi-swarm" / "recent-preview.json"


def recent():
    try:
        data = json.loads(state_path().read_text())
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def request(target, *, remember=True, project=None):
    result = subprocess.run(["/usr/bin/open", "--preview", str(target)],
                            text=True, capture_output=True, timeout=8)
    if result.returncode:
        raise ValueError(result.stderr.strip() or "Could not show the preview in ANGI Web.")
    if remember:
        try:
            atomic_write(state_path(), {"target": str(target), "project": str(project) if project else None})
        except OSError:
            pass  # Web already accepted the preview.
    return "Preview requested in ANGI Web."


def from_chat(root, path=None):
    if path is not None:
        return request(entry_point(root / path, root))
    home = Path.home().resolve()
    last = recent()
    target = last.get("target")
    folder = root if root != home else None
    if folder is None and (selected := os.environ.get("AGX_CHAT_PREVIEW_PROJECT")):
        from scope import validate_project
        try:
            folder = validate_project(selected)
        except (OSError, ValueError):
            pass
    if folder is not None:
        if isinstance(target, str) and last.get("project") == str(folder):
            return request(target, remember=False)
        try:
            page = entry_point(folder, folder)
        except (OSError, ValueError):
            if isinstance(target, str):
                return request(entry_point(target, folder))
            raise
        else:
            return request(page)
    if isinstance(target, str):
        # Re-show a desktop preview previously accepted for this account. Its file contents
        # are never returned to Chat and explicit new file paths still use the Chat scope.
        return request(target, remember=False)
    return request(entry_point(root, root))


def saved_in_chat(root, path):
    if receipt := os.environ.get("AGX_CHAT_PREVIEW_QUEUE"):
        with open(receipt, "a", encoding="utf-8") as stream:
            stream.write(json.dumps((str(root), path)) + "\n")
        return "ANGI Web will open the HTML preview when this turn finishes."
    return from_chat(root, path)


def show_chat_saves(receipt):
    """Open one entry page after the turn has finished writing its HTML and assets."""
    pages = []
    for line in receipt:
        try:
            root, relative = json.loads(line)
            root = Path(root)
            pages.append(entry_point(root / relative, root))
        except (OSError, ValueError, TypeError):
            continue
    if pages:
        try:
            request(preferred_page(pages))
        except (OSError, ValueError, subprocess.SubprocessError):
            pass


def from_terminal(argument):
    local = "://" in argument or argument.startswith(("localhost:", "127.", "[::1]:"))
    target = argument if local else entry_point(argument)
    project = Path.cwd().resolve() if local else None
    result = request(target, project=project)
    if os.environ.get("AGX_SWARM_RUN"):
        from messaging import current_run
        atomic_write(current_run() / "preview-shown.json", {"target": str(target)})
    return result


def after_report(run):
    if (run / "preview-shown.json").is_file():
        return
    from work_changes import CHANGES, read_record
    try:
        snapshot = read_record(run / CHANGES)
        root = Path(snapshot["root"])
        pages = [Path(path) for path, change in snapshot["files"].items()
                 if change["kind"] in ("Added", "Modified") and is_html(path)]
        if pages:
            # Prefer index pages, then the shallowest changed HTML file.
            page = preferred_page(pages)
            request(entry_point(root / page, root))
    except (OSError, ValueError, subprocess.SubprocessError):
        pass  # The work report remains available even without a graphical desktop.
