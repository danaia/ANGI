"""Serve one project folder to ANGI Web without exposing a file browser."""
import hmac
from http.cookies import CookieError, SimpleCookie
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import mimetypes
import os
import shutil
import stat
import sys
from urllib.parse import parse_qs, unquote, urlsplit


DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
TYPES = {
    ".html": "text/html; charset=utf-8", ".htm": "text/html; charset=utf-8",
    ".xhtml": "application/xhtml+xml", ".css": "text/css", ".js": "text/javascript",
    ".mjs": "text/javascript", ".json": "application/json", ".wasm": "application/wasm",
    ".webmanifest": "application/manifest+json", ".woff": "font/woff",
    ".woff2": "font/woff2", ".ttf": "font/ttf", ".otf": "font/otf",
}


def beneath(root, parts, flags):
    """Open a descendant by descriptor, rejecting links at every level."""
    fd = os.dup(root)
    try:
        for part in parts[:-1]:
            child = os.open(part, DIRECTORY, dir_fd=fd)
            os.close(fd)
            fd = child
        return os.open(parts[-1], flags, dir_fd=fd)
    finally:
        os.close(fd)


class Preview(BaseHTTPRequestHandler):
    def do_GET(self):
        self.serve_file()

    def do_HEAD(self):
        self.serve_file(head=True)

    def serve_file(self, head=False):
        server = self.server
        if (self.headers.get("Host") != f"127.0.0.1:{server.server_port}"
                or self.headers.get("Sec-Fetch-Site") in ("cross-site", "same-site")):
            self.send_error(403)
            return
        try:
            request = urlsplit(self.path)
            token = parse_qs(request.query).get("angi_preview", [""])[0]
            issue_cookie = hmac.compare_digest(token, server.token)
            cookies = SimpleCookie(self.headers.get("Cookie", ""))
            session = cookies.get(server.cookie)
            if not issue_cookie and (session is None or not hmac.compare_digest(session.value, server.token)):
                self.send_error(403)
                return
            parts = unquote(request.path).lstrip("/").split("/")
            if parts[-1] == "":
                parts[-1] = "index.html"
            if any(not part or part.startswith(".") or "\\" in part for part in parts):
                raise ValueError("Not a visible preview file")
            name = parts[-1]
            kind = TYPES.get(os.path.splitext(name.lower())[1]) or mimetypes.guess_type(name)[0]
            if not kind or not (name.lower().endswith(tuple(TYPES)) or kind.startswith(("image/", "audio/", "video/", "font/"))):
                raise ValueError("Not a web asset")
            fd = beneath(server.root, parts, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        except (OSError, ValueError, CookieError, TypeError):
            self.send_error(404)
            return
        with os.fdopen(fd, "rb") as data:
            info = os.fstat(data.fileno())
            if not stat.S_ISREG(info.st_mode):
                self.send_error(404)
                return
            if issue_cookie:
                self.send_response(303)
                self.send_header("Location", "/" + request.path.lstrip("/"))
                self.send_header("Set-Cookie", f"{server.cookie}={server.token}; Path=/; HttpOnly; SameSite=Strict")
                self.send_header("Referrer-Policy", "no-referrer")
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                return
            self.send_response(200)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(info.st_size))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Referrer-Policy", "no-referrer")
            self.end_headers()
            if not head:
                try:
                    shutil.copyfileobj(data, self.wfile)
                except (BrokenPipeError, ConnectionResetError):
                    pass


def main(root):
    # The desktop supplies a canonical path. Pin that directory without following a replacement link.
    filesystem = os.open("/", DIRECTORY)
    try:
        fd = beneath(filesystem, root.strip("/").split("/"), DIRECTORY)
    finally:
        os.close(filesystem)
    try:
        with ThreadingHTTPServer(("127.0.0.1", 0), Preview) as server:
            server.root = fd
            server.token = os.environ["AGX_PREVIEW_TOKEN"]
            server.cookie = f"angi_preview_{server.server_port}"
            print(server.server_port, flush=True)
            server.serve_forever()
    finally:
        os.close(fd)


if __name__ == "__main__":
    main(sys.argv[1])
