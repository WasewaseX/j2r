#!/usr/bin/env python3
"""Timing wrapper: run a command, print 'WALL_SECONDS PEAK_RSS_KB'.

With -o FILE the two numbers are written to FILE instead of stdout
(same contract as `/usr/bin/time -f '%e %M' -o FILE`).
Peak RSS comes from getrusage(RUSAGE_CHILDREN) — in this wrapper the
only reaped child is the measured command.
"""
import argparse
import os
import resource
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument("-o", dest="out")
p.add_argument("cmd", nargs=argparse.REMAINDER)
a = p.parse_args()
if a.cmd and a.cmd[0] == "--":
    a.cmd = a.cmd[1:]
t0 = time.monotonic()
rc = subprocess.run(a.cmd).returncode
wall = time.monotonic() - t0
rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss  # KB on Linux
line = f"{wall:.2f} {rss}"
if a.out:
    with open(a.out, "w") as f:
        f.write(line + "\n")
else:
    print(line, file=sys.stderr)
sys.exit(rc)
