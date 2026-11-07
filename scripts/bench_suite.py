#!/usr/bin/env python3
"""R5 benchmark harness — observe-only, never a gate.

Policy (docs/STRATEGIC_GAPS.md R5, locked): benchmarks observe; no
performance sentence without a number, a date, and a reproduction
command. This script produces exactly that: per fixture, the median of
N measured runs for each channel, machine metadata, and a CSV row set
appended to scripts/bench_results.csv.

Channels per fixture:
  javac   — one javac compile (reference side, informational)
  java    — wall time of the reference JVM run (median of N)
  j2r-gen — j2r build --release (translate .class -> crate + cargo build)
  native  — wall time of the generated native binary (median of N)

Usage:
  python3 scripts/bench_suite.py [--reps 5] [--out scripts/bench_results.csv] \
      [--fulljit] [--warmups 1] [--out-full scripts/bench_results_fulljit.csv] \
      Fixture1 Fixture2 ...

Fixture names are stems under crates/j2r-cli/tests/suite/. With no
arguments a default representative spread is used (a micro, a strings
case, a real 302-line program, a monitor handoff, and two thread
fixtures). The harness never asserts output equality — differential
correctness is gate-3's job; this measures cost, nothing else.

--fulljit adds a second java channel: the DEFAULT full tiered JVM
(C1 -> C2 profile-guided JIT) instead of only -XX:TieredStopAtLevel=1.
For steady-state workloads (runs long enough that JIT warmup is noise)
this is the honest "JIT compiler" comparison the R5 gap register asks
for. --fulljit rows go to --out-full (own header, self-consistent);
the legacy CSV schema is unchanged. --warmups raises the warmup count
for every channel (fulljit uses at least 2 — C2 needs hot loops to
 graduate, and a cold-C2 run would understate the JIT on purpose).

--native-reps / --native-warmups override the rep and warmup counts for
the NATIVE channel only. Reason: a steady-state native run takes
minutes (JamaBench ~274s measured), so a sandbox with a hard per-process
time cap cannot fit reps=5 + warmups in one session — the row records
exactly how many runs produced its native median, and the remaining
repetitions (same protocol, same binary) are recorded in
docs/BENCHMARKS.md alongside the row.
"""

import argparse
import datetime
import os
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SUITE = REPO / "crates" / "j2r-cli" / "tests" / "suite"
CORPUS = REPO / "corpus"

DEFAULT_FIXTURES = [
    "Arith",         # micro: pure arithmetic, negligible I/O
    "Strings",       # string building + concat (indy path)
    "BigRpg",        # real 302-line multi-class program (gate-5 subject)
    "WaitNotify",    # monitor handoff, two threads
    "ThreadHello",   # real thread spawn/join
    "ThreadSafepoint",  # threads + forced GC safepoints
]


def have(tool: str) -> bool:
    try:
        subprocess.run([tool, "-version"] if tool == "java" else [tool, "--version"],
                       capture_output=True, timeout=30)
        return True
    except Exception:
        return False


def cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except Exception:
        pass
    return platform.processor() or "unknown"


def median_run(cmd, cwd, reps, warmup=1):
    """Run cmd warmup+reps times; return (median_seconds, all_seconds)."""
    times = []
    for i in range(warmup + reps):
        t0 = time.perf_counter()
        # 600s cap: the steady-state native channel runs minutes per run
        # (JamaBench ~274s measured); 300s would leave too thin a margin.
        subprocess.run(cmd, cwd=cwd, capture_output=True, timeout=600)
        dt = time.perf_counter() - t0
        if i >= warmup:
            times.append(dt)
    return statistics.median(times), times


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("fixtures", nargs="*", help="stems under tests/suite")
    ap.add_argument("--reps", type=int, default=5)
    ap.add_argument("--out", default="scripts/bench_results.csv")
    ap.add_argument("--fulljit", action="store_true",
                    help="also measure the default full tiered JVM (C1->C2)")
    ap.add_argument("--warmups", type=int, default=1,
                    help="warmup runs before measurement (fulljit: >= 2)")
    ap.add_argument("--out-full", default="scripts/bench_results_fulljit.csv")
    ap.add_argument("--native-reps", type=int, default=None,
                    help="native-channel reps override (default: --reps)")
    ap.add_argument("--native-warmups", type=int, default=None,
                    help="native-channel warmup override (default: --warmups)")
    ap.add_argument("--emit-typed", action="store_true",
                    help="compile the native channel with --emit-typed (the "
                         "typed-operand backend). Row fixture names get a "
                         "[typed] suffix so the CSV rows are distinct.")
    args = ap.parse_args()
    full_warmups = max(2, args.warmups) if args.fulljit else args.warmups
    native_reps = args.native_reps if args.native_reps is not None else args.reps
    native_warmups = (args.native_warmups if args.native_warmups is not None
                      else args.warmups)

    if not have("java") or not have("javac"):
        print("java/javac not found — set JAVA_HOME/PATH first", file=sys.stderr)
        return 2
    if not have("cargo"):
        print("cargo not found", file=sys.stderr)
        return 2

    names = args.fixtures or DEFAULT_FIXTURES
    date = datetime.date.today().isoformat()
    stamp = datetime.datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
    rows = []

    print(f"# R5 bench — {stamp} — {cpu_model()} — {os.cpu_count()} cores — "
          f"{platform.machine()} — reps={args.reps} (median, "
          f"{full_warmups if args.fulljit else args.warmups} warmup(s)"
          f"{' , fulljit channel' if args.fulljit else ''})")
    if args.fulljit:
        header = (f"{'fixture':<16} {'javac(s)':>9} {'javaC1(ms)':>10} "
                  f"{'javaFull(ms)':>12} {'j2r-gen(s)':>10} "
                  f"{'native(ms)':>10} {'nat/C1':>8} {'nat/full':>8}")
    else:
        header = (f"{'fixture':<16} {'javac(s)':>9} {'java(ms)':>9} "
                  f"{'j2r-gen(s)':>10} {'native(ms)':>10} {'native/java':>12}")
    print(header)

    for name in names:
        src = SUITE / f"{name}.java"
        is_corpus = False
        if not src.exists():
            # Corpus entries (R5 policy: one bench row per corpus entry)
            # live under corpus/ with the same driver shape; their javac
            # output is a set of TOP-LEVEL sibling classes (the merge
            # strips the package), so the compile step feeds every
            # produced class, not just Name*.
            src = CORPUS / f"{name}.java"
            is_corpus = True
        if not src.exists():
            print(f"{name:<16} MISSING — skipped")
            continue
        work = Path("/tmp/j2r-bench") / name
        subprocess.run(["rm", "-rf", str(work)])
        work.mkdir(parents=True)
        env = dict(os.environ)
        # The corpus-scale generated crates (JZlib ~7.2k lines) SIGSEGV
        # rustc's default thread stack during expansion — see
        # docs/CORPUS_WAVE.md §Wave 2 build-side note.
        env.setdefault("RUST_MIN_STACK", "33554432")
        env.pop("J2R_SCHED", None)          # pure default mode, always
        env.pop("J2R_SCHED_SEED", None)
        env.pop("J2R_GC_EVERY_N", None)
        env.pop("J2R_CHAOS_US", None)

        # Reference compile (informational).
        t0 = time.perf_counter()
        subprocess.run(["javac", "-d", str(work / "classes"), str(src)],
                       capture_output=True, env=env, timeout=120)
        javac_s = time.perf_counter() - t0

        main_cls = name
        java_ms, _ = median_run(
            ["java", "-XX:TieredStopAtLevel=1", "-cp", str(work / "classes"), main_cls],
            cwd=REPO, reps=args.reps, warmup=args.warmups)
        java_ms *= 1000.0

        java_full_ms = None
        if args.fulljit:
            # The default JVM: full tiered JIT (C1 profiles -> C2 compiles
            # the hot loops). On a steady-state workload this is the real
            # JIT-compiler cost a Java user pays at run time.
            jf, _ = median_run(
                ["java", "-cp", str(work / "classes"), main_cls],
                cwd=REPO, reps=args.reps, warmup=full_warmups)
            java_full_ms = jf * 1000.0

        # j2r translate + release build (this is the compile-side cost).
        # The gate-5 proven invocation: all classes of the program, an
        # explicit --main-class, then an offline cargo build of the
        # generated crate (no --locked: generated crates carry no lock
        # file). Binary name follows the crate name j2r_app.
        cli_bin = REPO / "target" / "release" / "j2r"
        if not cli_bin.exists():
            subprocess.run(["cargo", "build", "--release", "-p", "j2r-cli", "--locked"],
                           cwd=REPO, capture_output=True, timeout=1200)
        if is_corpus:
            classes = sorted((work / "classes").glob("*.class"))
        else:
            classes = sorted((work / "classes").glob(f"{name}*.class"))
        t0 = time.perf_counter()
        gen_cmd = [str(cli_bin), "compile", *[str(c) for c in classes],
                   "--main-class", name, "--crate-name", "j2r_app",
                   "--out", str(work / "crate")]
        if args.emit_typed:
            gen_cmd.append("--emit-typed")
        gen = subprocess.run(gen_cmd,
            capture_output=True, env=env, cwd=REPO, timeout=1200)
        if gen.returncode != 0:
            print(f"{name:<16} COMPILE FAILED — skipped")
            continue
        gen_s = time.perf_counter() - t0  # translation only; native_total below

        # Native build of the generated crate (offline; runtime vendored).
        cargo_env = dict(env)
        cargo_env["CARGO_TARGET_DIR"] = str(work / "target")
        build = subprocess.run(
            ["cargo", "build", "--release", "--offline", "--manifest-path",
             str(work / "crate" / "Cargo.toml")],
            capture_output=True, env=cargo_env, timeout=1800)
        if build.returncode != 0:
            print(f"{name:<16} NATIVE BUILD FAILED — skipped")
            continue
        native_total = time.perf_counter() - t0  # translate + cargo build
        binary = work / "target" / "release" / "j2r_app"
        if not binary.exists():
            print(f"{name:<16} BINARY MISSING — skipped")
            continue

        native_ms, _ = median_run([str(binary)], cwd=REPO, reps=native_reps,
                                  warmup=native_warmups)
        native_ms *= 1000.0

        # The CSV keys rows by fixture name; a typed-backend run of the
        # same program must not look like a re-measurement of the default
        # emission. [typed] suffix = same law as the --emit-cfg addendum.
        row_name = f"{name}[typed]" if args.emit_typed else name

        if args.fulljit:
            r_c1 = native_ms / java_ms if java_ms > 0 else float("nan")
            r_full = native_ms / java_full_ms if java_full_ms > 0 else float("nan")
            print(f"{row_name:<16} {javac_s:>9.3f} {java_ms:>10.1f} "
                  f"{java_full_ms:>12.1f} {native_total:>10.2f} "
                  f"{native_ms:>10.1f} {r_c1:>8.2f} {r_full:>8.2f}")
            rows.append((row_name, f"{javac_s:.3f}", f"{java_ms:.1f}",
                         f"{java_full_ms:.1f}", f"{native_total:.2f}",
                         f"{native_ms:.1f}", f"{r_c1:.3f}", f"{r_full:.3f}"))
        else:
            ratio = native_ms / java_ms if java_ms > 0 else float("nan")
            print(f"{row_name:<16} {javac_s:>9.3f} {java_ms:>9.1f} "
                  f"{native_total:>10.2f} {native_ms:>10.1f} {ratio:>12.2f}x")
            rows.append((row_name, f"{javac_s:.3f}", f"{java_ms:.1f}",
                         f"{native_total:.2f}", f"{native_ms:.1f}", f"{ratio:.3f}"))

    if args.fulljit:
        out = REPO / args.out_full
        new_file = not out.exists()
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("a") as f:
            if new_file:
                f.write("date,cpu,cores,reps,warmups,fixture,javac_s,"
                        "java_c1_ms,java_full_ms,j2r_gen_s,native_ms,"
                        "native_over_java_c1,native_over_java_full,"
                        "native_reps,native_warmups\n")
            for r in rows:
                f.write(f"{stamp},{cpu_model()},{os.cpu_count()},{args.reps},"
                        f"{full_warmups},{r[0]},{r[1]},{r[2]},{r[3]},"
                        f"{r[4]},{r[5]},{r[6]},{r[7]},{native_reps},"
                        f"{native_warmups}\n")
        print(f"\nappended {len(rows)} fulljit rows to {args.out_full} "
              "(repro: see docs/BENCHMARKS.md)")
    else:
        out = REPO / args.out
        new_file = not out.exists()
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("a") as f:
            if new_file:
                f.write("date,cpu,cores,reps,fixture,javac_s,java_ms,j2r_gen_s,native_ms,native_over_java\n")
            for r in rows:
                f.write(f"{stamp},{cpu_model()},{os.cpu_count()},{args.reps},"
                        f"{r[0]},{r[1]},{r[2]},{r[3]},{r[4]},{r[5]}\n")
        print(f"\nappended {len(rows)} rows to {args.out} (repro: python3 {args.out.replace('scripts/', 'scripts/')} — "
              f"see docs/BENCHMARKS.md)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
