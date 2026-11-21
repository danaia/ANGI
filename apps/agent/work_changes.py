"""Before/after evidence and working-copy edits for an ANGI work record."""
from contextlib import contextmanager
import hashlib
import json
import os
import secrets
from pathlib import Path
import stat
import subprocess
import tempfile

from chat_files import DIRECTORY, directory
from scope import OS_SOURCE, validate_project

try:
    import gzip
except ImportError:  # Python built without zlib: keep records as plain JSON.
    gzip = None

FILE_LIMIT = 512 * 1024
SCAN_LIMIT = 48 * 1024 * 1024
EVIDENCE_LIMIT = 16 * 1024 * 1024
PATH_LIMIT = 12000
BASELINE = "before-work.json.gz"
CHANGES = "work-changes.json.gz"
GENERATED = {".git", ".cache", ".venv", "node_modules", "__pycache__", "target", "output"}


def workspace(mode, folder):
    return OS_SOURCE if mode == "os" else validate_project(folder)


def parts(path):
    if isinstance(path, str):
        names = path.split("/")
        if all(name and name not in {".", "..", ".git"} and "\0" not in name for name in names):
            return names
    raise ValueError("This path is outside the workspace.")


@contextmanager
def root_directory(root):
    fd = os.open(root, DIRECTORY)
    try:
        yield fd
    finally:
        os.close(fd)


def read_file(fd, name):
    handle = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
    with os.fdopen(handle, "rb") as stream:
        status = os.fstat(stream.fileno())
        if not stat.S_ISREG(status.st_mode):
            raise ValueError("This path is no longer a regular file.")
        return stream.read(FILE_LIMIT + 1), status


def decode(data):
    if len(data) <= FILE_LIMIT and b"\0" not in data:
        try:
            return data.decode("utf-8")
        except UnicodeDecodeError:
            pass


def candidates(root):
    try:
        output = subprocess.run(
            ["git", "-C", str(root), "-c", "core.fsmonitor=false", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "."],
            check=True, capture_output=True, timeout=15,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        for folder, dirs, files in os.walk(root):
            dirs[:] = sorted(set(dirs) - GENERATED)
            for name in sorted(files):
                yield (Path(folder) / name).relative_to(root).as_posix()
    else:
        for path in sorted(set(output.split(b"\0")) - {b""}):
            try:
                yield path.decode("utf-8")
            except UnicodeDecodeError:
                continue


def scan(root):
    files, total, limited = {}, 0, False
    with root_directory(root) as fd:
        for index, path in enumerate(candidates(root)):
            if index >= PATH_LIMIT:
                limited = True
                break
            try:
                path.encode("utf-8")
                *parents, name = parts(path)
                with directory(fd, parents) as parent:
                    data, _ = read_file(parent, name)
            except (OSError, ValueError):
                continue
            keep = len(data) <= FILE_LIMIT and total + len(data) <= SCAN_LIMIT
            files[path] = [hashlib.sha256(data).hexdigest() if keep else None, decode(data) if keep else None]
            limited |= not keep
            if keep:
                total += len(data)
    return files, limited


def write_record(path, data):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        staged = Path(stream.name)
        try:
            encoded = json.dumps(data, ensure_ascii=False).encode("utf-8")
            if gzip is None:
                stream.write(encoded)
            else:
                with gzip.GzipFile(fileobj=stream, mode="wb", compresslevel=3) as compressed:
                    compressed.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
            staged.replace(path)
        finally:
            staged.unlink(missing_ok=True)


def read_record(path):
    data = path.read_bytes()
    if data[:2] == b"\x1f\x8b":
        if gzip is None:
            raise FileNotFoundError(f"{path} is compressed but zlib is unavailable")
        data = gzip.decompress(data)
    return json.loads(data.decode("utf-8"))


def begin(run, root):
    try:
        files, limited = scan(root)
        write_record(run / BASELINE, {"root": str(root), "files": files, "limited": limited})
    except OSError as error:
        print(f"ANGI could not capture the starting files: {error}", flush=True)


def finish(run):
    try:
        before = read_record(run / BASELINE)
    except FileNotFoundError:
        return
    files = before["files"]
    after, limited = scan(Path(before["root"]))
    changes, total = {}, 0
    for path in sorted(files.keys() | after.keys()):
        old, new = files.get(path), after.get(path)
        if old == new:
            continue
        kind = "Added" if old is None else "Deleted" if new is None else "Modified"
        original = "" if old is None else old[1]
        recorded = "" if new is None else new[1]
        if original is not None and recorded is not None:
            size = len(original.encode("utf-8")) + len(recorded.encode("utf-8"))
            if total + size <= EVIDENCE_LIMIT:
                total += size
            else:
                original = recorded = None
                limited = True
        changes[path] = {"kind": kind, "original": original, "recorded": recorded}
    write_record(run / CHANGES, {"root": before["root"], "files": changes, "limited": limited or before["limited"]})


def evidence(run, root):
    try:
        record = read_record(run / CHANGES)
    except FileNotFoundError:
        return None
    return record if record["root"] == str(root) else None


def working_copy(root, path):
    *parents, name = parts(path)
    try:
        with root_directory(root) as fd, directory(fd, parents) as parent:
            data, info = read_file(parent, name)
            text = decode(data)
            if text is not None:
                return {"text": text, "revision": revision(data, info)}
    except (OSError, ValueError):
        pass
    return {"text": None, "revision": None}


def revision(data, info):
    return f"{info.st_dev}:{info.st_ino}:{info.st_mtime_ns}:" + hashlib.sha256(data).hexdigest()


def edit(root, path, text, expected):
    if not isinstance(text, str) or len(text.encode("utf-8")) > FILE_LIMIT or "\0" in text:
        raise ValueError("Save a UTF-8 text file under 512 KiB.")
    if not isinstance(expected, str):
        raise ValueError("Open a working copy before saving.")
    *parents, name = parts(path)
    with root_directory(root) as fd, directory(fd, parents) as parent:
        # Hold the parent descriptor across the check and same-directory replacement.
        data, info = read_file(parent, name)
        if revision(data, info) != expected or decode(data) is None:
            raise ValueError("The file changed on disk. Reload it before saving.")
        if info.st_uid != os.getuid() or not info.st_mode & stat.S_IWUSR:
            raise ValueError("The file is read-only for this account.")
        staged = ".angi-edit-" + secrets.token_hex(16)
        handle = os.open(staged, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=parent)
        try:
            with os.fdopen(handle, "wb") as stream:
                updated = text.encode("utf-8")
                stream.write(updated)
                stream.flush()
                os.fchmod(stream.fileno(), info.st_mode & 0o777)
                os.fsync(stream.fileno())
                result = revision(updated, os.fstat(stream.fileno()))
            current, status = read_file(parent, name)
            if revision(current, status) != expected:
                raise ValueError("The file changed on disk. Reload it before saving.")
            os.replace(staged, name, src_dir_fd=parent, dst_dir_fd=parent)
            return result
        finally:
            try:
                os.unlink(staged, dir_fd=parent)
            except FileNotFoundError:
                pass
