#!/usr/bin/env python3
"""Replay a seven-second pointer drag through an SSH VNC tunnel and sample HDMI cadence."""

import argparse
import csv
import math
from pathlib import Path
import re
import select
import shlex
import socket
import statistics
import struct
import subprocess
import time


def receive(connection, size):
    data = bytearray()
    while len(data) < size:
        block = connection.recv(size - len(data))
        if not block:
            raise ConnectionError("VNC closed the connection")
        data.extend(block)
    return data


def handshake(connection):
    if not receive(connection, 12).startswith(b"RFB "):
        raise ValueError("The SSH tunnel does not lead to a VNC server")
    connection.sendall(b"RFB 003.008\n")
    if 1 not in receive(connection, receive(connection, 1)[0]):
        raise ValueError("Expected ANGI's loopback VNC server behind SSH")
    connection.sendall(b"\x01")
    if receive(connection, 4) != b"\0" * 4:
        raise ValueError("VNC refused the connection")
    connection.sendall(b"\x01")
    initial = receive(connection, 24)
    receive(connection, struct.unpack_from("!I", initial, 20)[0])
    return struct.unpack_from("!HH", initial)


def drag(connection, x, y):
    def pointer(buttons, px, py):
        connection.sendall(struct.pack("!BBHH", 5, buttons, px, py))

    pointer(0, x, y)
    time.sleep(0.16)
    pointer(1, x, y)
    try:
        time.sleep(0.10)
        start = time.monotonic()
        for step in range(1, 7 * 120 + 1):
            elapsed = step / 120
            time.sleep(max(0, start + elapsed - time.monotonic()))
            angle = math.tau * elapsed
            pointer(1, x + round(110 * math.sin(angle)), y + round(30 * (1 - math.cos(angle))))
    finally:
        pointer(0, x, y)


def collect(ssh, command, port, x, y):
    with socket.create_connection(("127.0.0.1", port), timeout=10) as connection:
        connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        width, height = handshake(connection)
        if not (110 <= x < width - 110 and 0 <= y < height - 60):
            raise ValueError("Choose a title-bar point with room for the drag path")
        with subprocess.Popen([*ssh, command], stdout=subprocess.PIPE, bufsize=0) as probe:
            try:
                if not select.select([probe.stdout], [], [], 15)[0]:
                    raise TimeoutError("Timed out waiting for the DRM sampler")
                mode = probe.stdout.readline()
                if not mode.startswith(b"mode,"):
                    raise RuntimeError("The DRM sampler could not read the active display")
                time.sleep(0.55)
                drag(connection, x, y)
                remaining, _ = probe.communicate(timeout=12)
                if probe.returncode:
                    raise RuntimeError("The DRM sampler failed")
                return (mode + remaining).decode()
            finally:
                if probe.poll() is None:
                    probe.kill()


def summarize(sample):
    rows = list(csv.reader(sample.splitlines()))
    _, width, height, hz = rows[0]
    refresh = float(hz)
    frames = [float(row[1]) for row in rows if row[0] == "fb" and 1400 <= float(row[1]) <= 6900]
    gaps = sorted(b - a for a, b in zip(frames, frames[1:]))
    if not gaps:
        raise RuntimeError("No framebuffer changes during the drag. Check the selected title bar.")
    threshold = 1500 / refresh
    slow = sum(gap > threshold for gap in gaps)
    p95 = gaps[math.ceil(0.95 * len(gaps)) - 1]
    max_poll_gap = next(row[1] for row in rows if row[0] == "max_poll_gap_ms")
    return (f"{width}x{height} at {refresh:.2f} Hz, {1000 / statistics.mean(gaps):.2f} updates/s, "
            f"p95 {p95:.2f} ms, {slow}/{len(gaps)} gaps above {threshold:.2f} ms, "
            f"max polling gap {max_poll_gap} ms")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, help="Verified SSH account and host or SSH alias")
    parser.add_argument("--ssh-option", action="append", default=[], metavar="KEY=VALUE")
    parser.add_argument("--vnc-port", type=int, required=True, help="Local SSH forward to the device's VNC port")
    parser.add_argument("--card", required=True, help="DRM device on the target, such as /dev/dri/card1")
    parser.add_argument("--probe", type=Path, required=True, help="Local path to drm-cadence compiled for the target")
    parser.add_argument("--title", type=int, nargs=2, required=True, metavar=("X", "Y"))
    parser.add_argument("--output", type=Path, required=True, help="Directory for the raw probe CSVs")
    parser.add_argument("--runs", type=int, default=2)
    args = parser.parse_args()
    if args.runs < 1 or not re.fullmatch(r"/dev/dri/card\d+", args.card):
        parser.error("Choose at least one run and a valid DRM card path")
    ssh = ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", "-o", "ConnectTimeout=8"]
    for option in args.ssh_option:
        ssh.extend(("-o", option))
    ssh.extend(("--", args.host))
    scratch = subprocess.check_output([*ssh, "mktemp -d /tmp/angi-window-drag.XXXXXX"], text=True).strip()
    if not re.fullmatch(r"/tmp/angi-window-drag\.[A-Za-z0-9]{6}", scratch):
        raise RuntimeError("Unexpected temporary path on the device")
    remote_probe = shlex.quote(f"{scratch}/probe")
    try:
        args.output.mkdir(parents=True, exist_ok=True)
        subprocess.run([*ssh, f"cat > {remote_probe} && chmod 700 {remote_probe}"],
                       input=args.probe.read_bytes(), check=True)
        for run in range(1, args.runs + 1):
            path = args.output / f"drag-{run}.csv"
            sample = collect(ssh, f"{remote_probe} {args.card}", args.vnc_port, *args.title)
            with path.open("x") as data:
                data.write(sample)
            print(f"{path}: {summarize(sample)}", flush=True)
            time.sleep(1)
    finally:
        subprocess.run([*ssh, f"rm -f {remote_probe}; rmdir {shlex.quote(scratch)}"], check=True)


if __name__ == "__main__":
    main()
