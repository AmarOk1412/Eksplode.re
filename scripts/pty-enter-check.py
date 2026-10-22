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
import sys
import termios
import time

BIN = os.environ.get("BOMBERUST_CLI", "target/debug/bomberust-cli")
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
    host = spawn(["host", "--port", str(free_port()), "--name", "Host"])
    ok = wait_for([host[1]], host[1], "Slot 1")
    if ok:
        os.write(host[1], seq)
        ok = wait_for([host[1]], host[1], "bombs")
    stop(host)
    return ok


def check_joined_non_owner():
    port = free_port()
    host = spawn(["host", "--port", str(port), "--name", "Host"])
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
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
