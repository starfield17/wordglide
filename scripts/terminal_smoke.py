#!/usr/bin/env python3
"""Exercise real POSIX terminal input, automatic preview, and terminal restoration."""
import argparse
import fcntl
import os
import pty
import resource
import select
import struct
import subprocess
import sys
import termios
import time


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", default="target/release/wordglide")
    p.add_argument("--data", help="Omit to verify automatic adjacent-pack discovery")
    p.add_argument("--query", default="fist")
    p.add_argument("--needle", default="clenched", help="Expected source preview text")
    args = p.parse_args()
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    before = termios.tcgetattr(slave)
    command = [os.path.abspath(args.binary)]
    if args.data:
        command.extend(["--data", args.data])
    process = subprocess.Popen(command,
                               stdin=slave, stdout=slave, stderr=slave)
    output = bytearray()

    def drain(seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if select.select([master], [], [], min(0.05, max(0, end-time.monotonic())))[0]:
                output.extend(os.read(master, 65536))

    try:
        deadline = time.monotonic() + 120
        while b"\x1b[?1049h" not in output:
            drain(0.05)
            if process.poll() is not None or time.monotonic() > deadline:
                raise AssertionError("Dictionary did not enter its terminal UI")
        os.write(master, args.query.encode("utf-8"))
        deadline = time.monotonic() + 5
        needle = args.needle.encode("utf-8")
        while needle not in output and time.monotonic() < deadline:
            drain(0.05)
        if args.query.encode("utf-8") not in output or needle not in output:
            raise AssertionError("Typing did not produce the expected automatic definition preview")
        os.write(master, b"\x03")
        process.wait(timeout=5)
        drain(0.1)
        if process.returncode != 0:
            raise AssertionError(f"Dictionary exited with {process.returncode}")
        if termios.tcgetattr(slave) != before:
            raise AssertionError("Terminal attributes were not restored")
        if b"\x1b[?1049l" not in output:
            raise AssertionError("Alternate screen was not restored")
        print(f"PASS: real PTY typed {args.query} without Enter, previewed expected source text, and restored terminal state")
        rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
        rss_mib = rss / (1024 * 1024 if sys.platform == "darwin" else 1024)
        print(f"Peak dictionary process resident memory: {rss_mib:.1f} MiB (includes startup)")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
        os.close(slave)


if __name__ == "__main__":
    main()
