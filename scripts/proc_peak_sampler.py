#!/usr/bin/env python3
"""Per-process peak-memory sampler (G2.1 attribution probe).

Wraps a command and polls /proc every POLL_MS WHILE it runs, recording
each live process's VmHWM (high-water mark, /proc/<pid>/status). At the
end prints WALL_SECONDS, the overall max single-process VmHWM, and the
top-N processes by VmHWM — so a cargo build's peak can be attributed to
rustc, the linker, or a specific rustc invocation.

Usage: proc_peak_sampler.py [-o FILE] [-- cmd ...]
Output line 1: "WALL_SECONDS MAX_VMHWM_KB"
Then: top-N table "VmHWM_KB  pid  name".
"""
import argparse
import os
import subprocess
import sys
import time

TOP_N = 12
POLL_MS = 60


def snapshot(best):
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            with open(f"/proc/{pid}/status") as f:
                name = hwm = None
                for line in f:
                    if line.startswith("Name:"):
                        name = line.split(maxsplit=1)[1].strip()
                    elif line.startswith("VmHWM:"):
                        hwm = int(line.split()[1])
                        break
            if name and hwm:
                prev = best.get(pid)
                if prev is None or hwm > prev[0]:
                    best[pid] = (hwm, name)
        except (OSError, ValueError, IndexError):
            continue


def main():
    p = argparse.ArgumentParser()
    p.add_argument("-o", dest="out")
    p.add_argument("cmd", nargs=argparse.REMAINDER)
    a = p.parse_args()
    if a.cmd and a.cmd[0] == "--":
        a.cmd = a.cmd[1:]
    best = {}  # pid -> (hwm, name)
    t0 = time.monotonic()
    proc = subprocess.Popen(a.cmd)
    while proc.poll() is None:
        snapshot(best)
        time.sleep(POLL_MS / 1000.0)
    rc = proc.wait()
    wall = time.monotonic() - t0
    overall = max((h for h, _ in best.values()), default=0)
    print(f"WALL={wall:.2f}s MAX_SINGLE_PROC_VMHWM={overall} KB rc={rc}", flush=True)
    top = sorted(best.items(), key=lambda kv: -kv[1][0])[:TOP_N]
    for pid, (hwm, name) in top:
        print(f"  {hwm:>10} KB  pid={pid:<7} {name}", flush=True)
    if a.out:
        with open(a.out, "w") as f:
            f.write(f"{wall:.2f} {overall}\n")
            for pid, (hwm, name) in top:
                f.write(f"{hwm} {pid} {name}\n")
    return rc


if __name__ == "__main__":
    sys.exit(main())
