"""Local MCP tools for Chat's home folder or a folder the user selects."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import secrets
import stat
import subprocess
import sys

from scope import validate_project

DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
TEXT_LIMIT = 16_384
SAVE_LIMIT = 1_048_576
LIST_LIMIT = 100


@contextmanager
def directory(root_fd, parts, create=False):
    """Open each component relative to a pinned directory without following links."""
    fd = os.dup(root_fd)
    try:
        for part in parts:
            if create:
                try:
                    os.mkdir(part, dir_fd=fd)
                except FileExistsError:
                    pass
            child = os.open(part, DIRECTORY, dir_fd=fd)
            os.close(fd)
            fd = child
        yield fd
    finally:
        os.close(fd)


def relative_parts(path, at_root=False):
    if path == "." and at_root:
        return []
    if isinstance(path, str):
        parts = path.split("/")
        if all(part and not part.startswith(".") and "\0" not in part for part in parts):
            return parts
    raise ValueError("Use a relative path to a visible file or directory inside the Chat workspace.")


def list_folder(root_fd, path):
    with directory(root_fd, relative_parts(path, at_root=True)) as fd, os.scandir(fd) as entries:
        visible = []
        for entry in entries:
            if entry.name.startswith(".") or entry.is_symlink():
                continue
            if entry.is_dir(follow_symlinks=False):
                visible.append(entry.name + "/")
            elif entry.is_file(follow_symlinks=False):
                visible.append(entry.name)
            if len(visible) > LIST_LIMIT:
                break
    return json.dumps({"entries": sorted(visible[:LIST_LIMIT]), "truncated": len(visible) > LIST_LIMIT})


def read_text(root_fd, path):
    *parents, name = relative_parts(path)
    with directory(root_fd, parents) as fd:
        file = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        with os.fdopen(file, "rb") as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                raise ValueError("Choose a regular text file.")
            data = stream.read(TEXT_LIMIT + 1)
    if len(data) > TEXT_LIMIT:
        return data[:TEXT_LIMIT].decode("utf-8", errors="replace") + "\n[File truncated]"
    return data.decode("utf-8")


def make_folder(root_fd, path):
    with directory(root_fd, relative_parts(path), create=True):
        return f"Folder ready: {path}"


def save_text(root_fd, path, text, overwrite=False):
    if not isinstance(text, str) or not isinstance(overwrite, bool):
        raise ValueError("Provide text and an optional overwrite boolean.")
    data = text.encode("utf-8")
    if len(data) > SAVE_LIMIT:
        raise ValueError("The text is too large to save in one call.")
    *parents, name = relative_parts(path)
    with directory(root_fd, parents) as fd:
        mode = 0o600
        if overwrite:
            try:
                current = os.stat(name, dir_fd=fd, follow_symlinks=False)
            except FileNotFoundError:
                pass
            else:
                if not stat.S_ISREG(current.st_mode):
                    raise ValueError("Only a regular text file can be replaced.")
                mode = current.st_mode & 0o777
        temporary = ".angi-save-" + secrets.token_hex(16)
        file = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=fd)
        try:
            with os.fdopen(file, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fchmod(stream.fileno(), mode)
                os.fsync(stream.fileno())
            if overwrite:
                os.replace(temporary, name, src_dir_fd=fd, dst_dir_fd=fd)
            else:
                os.link(temporary, name, src_dir_fd=fd, dst_dir_fd=fd, follow_symlinks=False)
        finally:
            try:
                os.unlink(temporary, dir_fd=fd)
            except FileNotFoundError:
                pass
    return f"Saved: {path}"


def preview(root_fd, path=None):
    from agent_preview import from_chat
    root = Path(os.readlink(f"/proc/self/fd/{root_fd}"))
    if path is not None:
        path = "/".join(relative_parts(path, at_root=True)) or "."
    return from_chat(root, path)


PATH = {"type": "string", "description": "Path relative to the Chat workspace."}
TOOLS = {
    "list": (list_folder, "List a directory. Use . for the Chat workspace.", {"path": PATH}, ["path"]),
    "read": (read_text, "Read a UTF-8 text file.", {"path": PATH}, ["path"]),
    "mkdir": (make_folder, "Create a directory and any missing parent directories.", {"path": PATH}, ["path"]),
    "save": (save_text, "Save UTF-8 text. Create the parent directory first.", {
        "path": PATH, "text": {"type": "string"},
        "overwrite": {"type": "boolean", "description": "Set true only if the user requested changing an existing file."},
    }, ["path", "text"]),
    "preview": (preview, "Show HTML or a web project beside Chat in ANGI Web. Omit path to show the current or most recently previewed project.", {"path": PATH}, []),
}


def respond(root_fd, request, *, show_saved=False):
    response = {"jsonrpc": "2.0", "id": request["id"]}
    method = request.get("method")
    params = request.get("params", {})
    if method == "initialize":
        result = {"protocolVersion": "2025-03-26", "capabilities": {"tools": {}},
                  "serverInfo": {"name": "angi-chat-files", "version": "1.0"}}
    elif method == "ping":
        result = {}
    elif method == "tools/list":
        result = {"tools": [
            {"name": name, "description": description, "inputSchema": {
                "type": "object", "properties": properties, "required": required, "additionalProperties": False,
            }}
            for name, (_, description, properties, required) in TOOLS.items()
        ]}
    elif method == "tools/call":
        try:
            name = params["name"]
            arguments = params.get("arguments", {})
            action = TOOLS[name][0]
            text = action(root_fd, **arguments)
            if show_saved and name == "save":
                from agent_preview import is_html, saved_in_chat
                if is_html(arguments["path"]):
                    try:
                        root = Path(os.readlink(f"/proc/self/fd/{root_fd}"))
                        text += "\n" + saved_in_chat(root, arguments["path"])
                    except (OSError, ValueError, subprocess.SubprocessError) as exc:
                        text += f"\nANGI Web could not show the preview: {exc}"
            error = False
        except (OSError, TypeError, ValueError, KeyError, subprocess.SubprocessError) as exc:
            text = str(exc) if not isinstance(exc, OSError) else exc.strerror
            error = True
        result = {"content": [{"type": "text", "text": text}], "isError": error}
    else:
        return {**response, "error": {"code": -32601, "message": "Method not found"}}
    return {**response, "result": result}


def serve(folder):
    root = validate_project(folder, allow_home=True)
    if str(root) != folder:
        raise ValueError("The selected folder has moved. Select it again.")
    filesystem = os.open("/", DIRECTORY)
    try:
        with directory(filesystem, root.parts[1:]) as root_fd:
            for line in sys.stdin:
                request = json.loads(line)
                if "id" in request:
                    print(json.dumps(respond(root_fd, request, show_saved=True)), flush=True)
    finally:
        os.close(filesystem)


if __name__ == "__main__":
    serve(sys.argv[1])
