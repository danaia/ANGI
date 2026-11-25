#!/usr/bin/env python3
"""Fetch one frame from a VNC server and save it as a PNG: a check that a remote desktop really
shows the screen. Standard library only; asks for Raw encoding and no authentication, which is
what ANGI's loopback-only server offers behind its SSH tunnel.

    scripts/vnc-snapshot.py 127.0.0.1 5941 frame.png
"""
import socket
import struct
import sys
import zlib


def read(sock, count):
    data = b""
    while len(data) < count:
        chunk = sock.recv(count - len(data))
        if not chunk:
            raise ConnectionError("the server closed the connection")
        data += chunk
    return data


def png(path, width, height, rgb):
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    rows = b"".join(b"\x00" + rgb[y * width * 3:(y + 1) * width * 3] for y in range(height))
    with open(path, "wb") as out:
        out.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                  + chunk(b"IDAT", zlib.compress(rows, 6)) + chunk(b"IEND", b""))


def snapshot(host, port, path):
    sock = socket.create_connection((host, port), timeout=10)
    greeting = read(sock, 12)
    if not greeting.startswith(b"RFB "):
        raise ValueError(f"not a VNC server: {greeting!r}")
    sock.sendall(b"RFB 003.008\n")
    types = read(sock, read(sock, 1)[0])
    if types and types[0] == 0:
        raise ValueError(read(sock, struct.unpack(">I", read(sock, 4))[0]).decode(errors="replace"))
    if 1 not in types:
        raise ValueError(f"the server requires authentication (security types {list(types)})")
    sock.sendall(b"\x01")
    if struct.unpack(">I", read(sock, 4))[0] != 0:
        raise ValueError("the server refused the security handshake")
    sock.sendall(b"\x01")  # ClientInit: share the desktop
    width, height = struct.unpack(">HH", read(sock, 4))
    bpp, _depth, big, _true, rmax, gmax, bmax, rsh, gsh, bsh = struct.unpack(">BBBBHHHBBB", read(sock, 13))
    read(sock, 3)
    name = read(sock, struct.unpack(">I", read(sock, 4))[0]).decode(errors="replace")
    print(f"connected to {name!r}: {width}x{height}, {bpp} bits per pixel")
    sock.sendall(struct.pack(">BxHi", 2, 1, 0))  # SetEncodings: Raw only
    sock.sendall(struct.pack(">BBHHHH", 3, 0, 0, 0, width, height))
    frame = bytearray(width * height * 3)
    covered = 0
    while covered < width * height:
        kind = read(sock, 1)[0]
        if kind != 0:
            raise ValueError(f"unexpected server message {kind}")
        _, rectangles = struct.unpack(">xH", read(sock, 3))
        for _ in range(rectangles):
            x, y, w, h, encoding = struct.unpack(">HHHHi", read(sock, 12))
            if encoding != 0:
                raise ValueError(f"the server sent encoding {encoding}; only Raw is handled")
            data = read(sock, w * h * bpp // 8)
            for row in range(h):
                for column in range(w):
                    offset = (row * w + column) * (bpp // 8)
                    pixel = int.from_bytes(data[offset:offset + bpp // 8], "big" if big else "little")
                    at = ((y + row) * width + x + column) * 3
                    frame[at] = ((pixel >> rsh) & rmax) * 255 // rmax
                    frame[at + 1] = ((pixel >> gsh) & gmax) * 255 // gmax
                    frame[at + 2] = ((pixel >> bsh) & bmax) * 255 // bmax
            covered += w * h
    png(path, width, height, bytes(frame))
    distinct = len(set(frame[i:i + 3] for i in range(0, len(frame), 3 * 997)))
    print(f"saved {path} ({width}x{height}); about {distinct} distinct colours sampled")
    sock.close()


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    try:
        snapshot(sys.argv[1], int(sys.argv[2]), sys.argv[3])
    except (OSError, ValueError, ConnectionError) as error:
        sys.exit(f"vnc-snapshot: {error}")
