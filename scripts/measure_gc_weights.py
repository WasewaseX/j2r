#!/usr/bin/env python3
"""Measure per-fixture runtime under J2R_GC_EVERY_N=1 (gate-9 semantics).

CI #59 proved stride sharding balances counts, not durations (legs
14m/1m/>=120m/1m/1m/>=120m on one tree). This script produces the honest
per-fixture weight table for duration-balanced explicit gate-9 buckets.

Sandbox reality: background processes die between tool calls, so the run
is CHUNKED and RESUMABLE — call it repeatedly (foreground) until it
reports ALL DONE:

  python3 scripts/measure_gc_weights.py --phase a --window 540   # light, cap 60s
  python3 scripts/measure_gc_weights.py --phase b --window 540   # suspects, cap 540s

Results append to scripts/gc_weights.csv (checkpoint after EVERY fixture;
re-runs skip already-measured names). Phase B runs 2 fixtures in parallel
(2-core box, each gets a core; timings within ~15% of serial).

Every measurement is the real differential pipeline (`j2r test` = javac
compile, JVM run, j2r compile, native run, three-channel compare), so
each row includes ~3-5s constant overhead. A phase-A TIMEOUT_60 row
means the fixture is heavier than classified — phase B re-measures it
with the long cap (phase A timeouts are auto-requeued).
"""

import argparse
import csv
import multiprocessing as mp
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SUITE = os.path.join(ROOT, "crates/j2r-cli/tests/suite")
J2R = os.path.join(ROOT, "target/release/j2r")
OUT = os.path.join(ROOT, "scripts/gc_weights.csv")

ENV = dict(os.environ)
ENV.pop("CARGO_TARGET_DIR", None)  # must never leak into j2r test inner builds
ENV["J2R_GC_EVERY_N"] = "1"
ENV["JAVA_HOME"] = "/home/z/tools/jdk-21.0.13+9"
ENV["PATH"] = "/home/z/tools/jdk-21.0.13+9/bin:" + ENV.get("PATH", "")

SUSPECTS = {
    "BigCache", "BigGradebook", "BigHuffman", "BigJson", "BigLang",
    "BigRpg", "Cycles", "GcIdentity", "HashSetOps", "HashMapOrder",
    "InternAcrossGc", "LambdaGC", "LambdaInCollections", "MonitorContention",
    "ReflectGC", "ForEachColl", "ThreadHello", "ThreadSafepoint",
    "WaitNotify", "WaitTimeout",
}


def run_one(name: str, cap: int):
    fixture = os.path.join(SUITE, name + ".java")
    start = time.monotonic()
    try:
        proc = subprocess.run(
            [J2R, "test", fixture],
            capture_output=True, text=True, timeout=cap, env=ENV,
        )
        dt = time.monotonic() - start
        status = "pass" if proc.returncode == 0 else f"FAIL({proc.returncode})"
        if status.startswith("FAIL"):
            sys.stderr.write(f"FAIL {name}\n--- stderr tail ---\n"
                             + proc.stderr[-1500:] + "\n")
    except subprocess.TimeoutExpired:
        dt = float(cap)
        status = f"TIMEOUT_{cap}"
    return name, round(dt, 1), status


def load_done():
    done = {}
    if os.path.exists(OUT):
        with open(OUT) as fh:
            for row in csv.DictReader(fh):
                done[row["name"]] = (float(row["seconds"]), row["status"])
    return done


def append_row(name, seconds, status):
    new_file = not os.path.exists(OUT)
    with open(OUT, "a", newline="") as fh:
        w = csv.writer(fh)
        if new_file:
            w.writerow(["name", "seconds", "status"])
        w.writerow([name, seconds, status])


def phase_a(window):
    done = load_done()
    fixtures = sorted(f[:-5] for f in os.listdir(SUITE) if f.endswith(".java"))
    todo = [f for f in fixtures
            if f not in SUSPECTS
            and (f not in done or done[f][1].startswith("TIMEOUT"))]
    # Phase-A timeouts that were already measured get re-measured here too
    # (they were misclassified as light; they move to the long-cap class).
    print(f"Phase A: {len(todo)} to measure (cap 60s, window {window}s)")
    t0 = time.monotonic()
    measured = 0
    for name in todo:
        if time.monotonic() - t0 > window - 65:
            break
        row = run_one(name, 60)
        append_row(*row)
        print(f"  {row[0]}: {row[1]}s {row[2]}", flush=True)
        measured += 1
    print(f"Phase A: measured {measured} this window; "
          f"remaining after: {len(todo) - measured}")


BATCH = 2


def phase_b_one(args):
    return run_one(args, 540)


def phase_b(window):
    done = load_done()
    fixtures = sorted(f[:-5] for f in os.listdir(SUITE) if f.endswith(".java"))
    todo = []
    for f in fixtures:
        if f not in SUSPECTS:
            st = done.get(f, ("", ""))[1]
            if st.startswith("TIMEOUT"):
                todo.append(f)  # phase-A misclassification -> re-measure long
        else:
            if f not in done or done[f][1].startswith("TIMEOUT_60"):
                todo.append(f)
    print(f"Phase B: {len(todo)} to measure (cap 540s, 2 workers)")
    if not todo:
        return
    t0 = time.monotonic()
    batch = todo[:BATCH]
    with mp.Pool(2) as pool:
        for row in pool.imap_unordered(phase_b_one, batch):
            append_row(*row)
            print(f"  {row[0]}: {row[1]}s {row[2]}", flush=True)
    print(f"Phase B: {len(todo) - len(batch)} remaining after this window")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase", choices=["a", "b"], required=True)
    ap.add_argument("--window", type=int, default=540)
    args = ap.parse_args()
    (phase_a if args.phase == "a" else phase_b)(args.window)


if __name__ == "__main__":
    main()
