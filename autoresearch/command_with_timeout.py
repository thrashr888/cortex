#!/usr/bin/env python3
import os
import signal
import subprocess
import sys


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: command_with_timeout.py SECONDS CMD [ARGS...]", file=sys.stderr)
        return 2

    timeout = int(sys.argv[1])
    cmd = sys.argv[2:]

    proc = subprocess.Popen(cmd)
    try:
        return proc.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            proc.terminate()
            return proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        return 124


if __name__ == "__main__":
    raise SystemExit(main())
