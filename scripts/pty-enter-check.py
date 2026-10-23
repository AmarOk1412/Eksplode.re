#!/usr/bin/env python3
"""Runs bomberust-cli in pseudo-terminals and checks that every kind of Enter works.

Usage: scripts/pty-enter-check.py [--attempts N]
The binary is taken from $BOMBERUST_CLI or target/debug/bomberust-cli (build it first).
Exit status 0 when every check passes.
"""
import fcntl
import os
import pty
import re
import select
import socket
import struct
import subprocess
import sys
import termios
import time
import urllib.request

BIN = os.environ.get("BOMBERUST_CLI", "target/debug/bomberust-cli")
SERVER = os.environ.get("BOMBERUST_SERVER", "target/debug/bomberust-server")
ENTERS = {
    "CR": b"\r",
    "LF": b"\n",
    "CRLF": b"\r\n",
    "kitty": b"\x1b[13u",
}
bufs = {}


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def spawn(args):
    pid, fd = pty.fork()
    if pid == 0:
        os.execv(BIN, ["bomberust-cli"] + args)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    return pid, fd


def pump(fds, seconds):
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select(fds, [], [], 0.05)
        for fd in ready:
            try:
                bufs[fd] = bufs.get(fd, b"") + os.read(fd, 65536)
            except OSError:
                pass


def text(fd):
    return re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z]", b"", bufs.get(fd, b"")).decode(errors="ignore")


def wait_for(fds, fd, needle, seconds=3.0):
    end = time.time() + seconds
    while time.time() < end:
        pump(fds, 0.1)
        if needle in text(fd):
            return True
    return False


def stop(*procs):
    for pid, fd in procs:
        try:
            os.write(fd, b"q")
        except OSError:
            pass
    time.sleep(0.3)
    for pid, fd in procs:
        for sig in (15, 9):
            try:
                os.kill(pid, sig)
            except OSError:
                break
        try:
            os.waitpid(pid, 0)
        except OSError:
            pass


def check_host_enter(label, seq):
    host = spawn(["host", "--no-setup", "--port", str(free_port()), "--name", "Host"])
    ok = wait_for([host[1]], host[1], "Slot 1")
    if ok:
        os.write(host[1], seq)
        ok = wait_for([host[1]], host[1], "bombs")
    stop(host)
    return ok


def start_server():
    """A headless bomberust-server; returns (process, game_port, http_port)."""
    game, http = free_port(), free_port()
    proc = subprocess.Popen(
        [SERVER, "--game-port", str(game), "--http-port", str(http)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    end = time.time() + 5
    while time.time() < end:
        try:
            socket.create_connection(("127.0.0.1", game), timeout=0.2).close()
            return proc, game, http
        except OSError:
            time.sleep(0.05)
    proc.kill()
    raise RuntimeError("bomberust-server did not start")


def check_owner_through_server(seq):
    """The first terminal player of a headless server is the owner and can start."""
    proc, game, _ = start_server()
    try:
        client = spawn(["join", f"127.0.0.1:{game}", "--name", "Own"])
        ok = wait_for([client[1]], client[1], "Slot 1")
        if ok:
            os.write(client[1], seq)
            ok = wait_for([client[1]], client[1], "bombs")
        stop(client)
        return ok
    finally:
        proc.terminate()
        proc.wait()


def check_host_serves_the_page():
    """host --web-port shows the page address and really serves the page."""
    web = free_port()
    host = spawn(["host", "--no-setup", "--port", str(free_port()), "--web-port", str(web), "--bind", "127.0.0.1"])
    url = f"http://127.0.0.1:{web}/"
    ok = wait_for([host[1]], host[1], url)
    if ok:
        try:
            with urllib.request.urlopen(url, timeout=3) as r:
                ok = r.status == 200 and b"Bomberust" in r.read()
        except OSError:
            ok = False
    stop(host)
    return ok


def check_joined_non_owner():
    port = free_port()
    host = spawn(["host", "--no-setup", "--port", str(port), "--name", "Host"])
    wait_for([host[1]], host[1], "Slot 1")
    client = spawn(["join", f"127.0.0.1:{port}", "--name", "Cli"])
    fds = [host[1], client[1]]
    ok = wait_for(fds, client[1], "Slot 2")
    if ok:
        os.write(client[1], b"\r")
        ok = wait_for(fds, client[1], "Only Host")
    stop(client, host)
    return ok


def main():
    attempts = 1
    if "--attempts" in sys.argv:
        attempts = int(sys.argv[sys.argv.index("--attempts") + 1])
    if not os.path.exists(BIN):
        print(f"missing {BIN}; run cargo build -p bomberust-cli first")
        return 2
    failures = 0
    for label, seq in ENTERS.items():
        results = [check_host_enter(label, seq) for _ in range(attempts)]
        good = all(results)
        failures += 0 if good else 1
        print(f"host lobby, {label}: {'ok' if good else 'FAILED'} ({sum(results)}/{attempts})")
    good = check_joined_non_owner()
    failures += 0 if good else 1
    print(f"joined non-owner sees 'Only Host ...': {'ok' if good else 'FAILED'}")
    if os.path.exists(SERVER):
        for label, seq in ENTERS.items():
            results = [check_owner_through_server(seq) for _ in range(attempts)]
            good = all(results)
            failures += 0 if good else 1
            print(f"joined owner on bomberust-server, {label}: {'ok' if good else 'FAILED'} ({sum(results)}/{attempts})")
    else:
        print(f"skipped the bomberust-server checks: {SERVER} is not built")
    good = check_host_serves_the_page()
    failures += 0 if good else 1
    print(f"host --web-port serves the page: {'ok' if good else 'FAILED'}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
