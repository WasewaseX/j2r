#!/bin/bash
# G2.1 checkbox closer — 20k peak-rustc re-measurement of the SPLIT
# emitter, same protocol as the 2026-11-09 baseline (console.md §20k
# census-jar confirmation): N=20000, k=250 → 80 part crates + support +
# app; cargo -j 2; CARGO_INCREMENTAL=0; profile dev with debug=0;
# RUSAGE_CHILDREN peak (getrusage max single child, timeit.py).
#
# Sandbox lesson #9: background processes are reaped when a tool call
# ends — long builds run as FOREGROUND calls ≤10 min. This script is
# therefore STAGE-BASED; call one stage per tool call and let cargo
# fingerprints resume the cold build across chunks. Wall time is summed
# across chunks in the stage5 report and recorded honestly (killed
# chunks leave no .time file — the per-chunk log line is the honesty
# record).
#
# Stages:
#   stage0  farm gen (if missing) + javac + jar          (~3 min)
#   stage1  j2r emit, cold (manifest created)            (~1 min)
#   stage2  ONE cargo chunk of the COLD build            (≤10 min, repeat
#           until $WORK/build1.cargo.DONE exists)
#   stage3  no-op rebuild: emit + cargo                  (~1 min)
#   stage4  one-body-edit (C00042) rebuild: jar + emit + cargo (~1 min)
#   stage5  parity vs Temurin + footprint + raw evidence copy
#
# Usage: g2_1_20k_remeasure.sh stage<N>
set -uo pipefail
source /home/z/my-project/env.sh
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=33554432

REPO=/home/z/my-project/work/j2r-fresh
WORK=/home/z/my-project/work/m5invest/farm20k
N=20000
K=250
LOG="$WORK/remeasure.log"
mkdir -p "$WORK"
exec >> "$LOG" 2>&1
say() { echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*"; }

J2R="$REPO/target/release/j2r"
OUT="$WORK/out20k"
JAR="$WORK/farm$N.jar"
SRC="$WORK/farm_src"

case "${1:?usage: g2_1_20k_remeasure.sh stage<N>}" in
stage0)
  say "================ G2.1 20k RE-MEASURE START (N=$N k=$K debug=0 -j2 CI=0) — split emitter"
  df -h / | tail -1
  if [ ! -f "$SRC/sources.txt" ]; then
    python3 "$REPO/scripts/m5_farm_gen.py" "$N" "$WORK" || { say "GEN FAILED"; exit 1; }
  fi
  cd "$SRC"
  if [ ! -d classes ]; then
    python3 "$REPO/scripts/timeit.py" -o "$WORK/javac.time" -- \
      javac -d classes @sources.txt 2> javac.err || { say "JAVAC FAILED"; tail -3 javac.err; exit 1; }
    say "javac WALL_S PEAK_RSS_KB = $(cat "$WORK/javac.time")"
  fi
  if [ ! -f "$JAR" ]; then
    jar cf "$JAR" -C "$SRC/classes" farm || { say "JAR FAILED"; exit 1; }
  fi
  say "stage0 OK: jar $(du -sh "$JAR" | cut -f1)"
  ;;

stage1)
  say "== [1/5] COLD emit (fresh out dir)"
  rm -rf "$OUT"
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build1.compile.time" -- \
    "$J2R" compile --classpath "$JAR" --main-class farm.Main \
    --crate-name farm20000 --out "$OUT" --partition-classes "$K" \
    > "$WORK/build1.compile.log" 2>&1 || { say "EMIT FAILED"; tail -5 "$WORK/build1.compile.log"; exit 1; }
  say "cold emit WALL_S PEAK_RSS_KB = $(cat "$WORK/build1.compile.time")"
  grep -E "manifest:" "$WORK/build1.compile.log" | tail -1 || true
  ;;

stage2)
  CH=$(cat "$WORK/cargo.chunk.next" 2>/dev/null || echo 1)
  say "== [2/5] COLD cargo chunk #$CH (fingerprints resume)"
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build1.cargo.chunk$CH.time" -- \
    cargo build --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build1.cargo.chunk$CH.log" 2>&1
  RC=$?
  if [ $RC -eq 0 ]; then
    say "cold cargo DONE in chunks (last chunk WALL_S PEAK_RSS_KB = $(cat "$WORK/build1.cargo.chunk$CH.time"))"
    touch "$WORK/build1.cargo.DONE"
  else
    say "chunk #$CH ended rc=$RC (timeout-kill expected mid-build; fingerprints resume)"
  fi
  echo $((CH + 1)) > "$WORK/cargo.chunk.next"
  ;;

stage3)
  say "== [3/5] NO-OP rebuild (nothing changed)"
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build2.compile.time" -- \
    "$J2R" compile --classpath "$JAR" --main-class farm.Main \
    --crate-name farm20000 --out "$OUT" --partition-classes "$K" \
    > "$WORK/build2.compile.log" 2>&1 || { say "NOOP EMIT FAILED"; exit 1; }
  say "no-op emit WALL_S PEAK_RSS_KB = $(cat "$WORK/build2.compile.time")"
  grep -E "manifest:" "$WORK/build2.compile.log" | tail -1 || true
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build2.cargo.time" -- \
    cargo build --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build2.cargo.log" 2>&1 || { say "NOOP CARGO FAILED"; exit 1; }
  say "no-op cargo WALL_S PEAK_RSS_KB = $(cat "$WORK/build2.cargo.time")"
  ;;

stage4)
  say "== [4/5] ONE-CLASS BODY EDIT (C00042 constant tweaked, same signature)"
  python3 - <<'EOF'
p = '/home/z/my-project/work/m5invest/farm20k/farm_src/farm/C00042.java'
src = open(p).read()
src2 = src.replace('seed + i *', 'seed + i * 2 *', 1)
assert src2 != src, "tweak must change the file"
open(p, 'w').write(src2)
EOF
  javac -d "$SRC/classes" -cp "$SRC/classes" "$SRC/farm/C00042.java" \
    || { say "EDIT JAVAC FAILED"; exit 1; }
  jar cf "$JAR" -C "$SRC/classes" farm
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build3.compile.time" -- \
    "$J2R" compile --classpath "$JAR" --main-class farm.Main \
    --crate-name farm20000 --out "$OUT" --partition-classes "$K" \
    > "$WORK/build3.compile.log" 2>&1 || { say "EDIT EMIT FAILED"; exit 1; }
  say "one-edit emit WALL_S PEAK_RSS_KB = $(cat "$WORK/build3.compile.time")"
  grep -E "manifest:" "$WORK/build3.compile.log" | tail -1 || true
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build3.cargo.time" -- \
    cargo build --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build3.cargo.log" 2>&1 || { say "EDIT CARGO FAILED"; exit 1; }
  say "one-edit cargo WALL_S PEAK_RSS_KB = $(cat "$WORK/build3.cargo.time")"
  ;;

stage5)
  say "== [5/5] parity vs Temurin + footprint + evidence copy"
  BIN=$(find "$OUT/target/debug" -maxdepth 1 -type f -perm -111 \
        \( -name "farm20000" -o -name "farm_20000" \) | head -1)
  say "binary: $BIN ($(stat -c%s "$BIN") B)"
  "$BIN" > "$WORK/rust.out" 2> "$WORK/rust.err"; RC=$?
  java -cp "$SRC/classes" farm.Main > "$WORK/java.out" 2> "$WORK/java.err"; RJ=$?
  say "exit codes: rust=$RC java=$RJ"
  if diff -q "$WORK/rust.out" "$WORK/java.out" >/dev/null \
     && diff -q "$WORK/rust.err" "$WORK/java.err" >/dev/null \
     && [ "$RC" = "$RJ" ]; then
    say "PARITY: PASS (stdout+stderr+exit identical)"
    echo "PASS" > "$WORK/PARITY"
  else
    say "PARITY: FAIL — head diff below"
    diff "$WORK/java.out" "$WORK/rust.out" | head -5 || true
    echo "FAIL" > "$WORK/PARITY"
  fi
  # cargo cold wall = sum across chunks (killed chunks logged honestly)
  TOT=0; PEAK=0
  for f in "$WORK"/build1.cargo.chunk*.time; do
    [ -e "$f" ] || continue
    W=$(cut -d' ' -f1 "$f"); P=$(cut -d' ' -f2 "$f")
    TOT=$(python3 -c "print($TOT + $W)")
    PEAK=$(python3 -c "print(max($PEAK, $P))")
  done
  say "cold cargo TOTAL wall across chunks = ${TOT}s ; PEAK single process = ${PEAK} KB"
  say "out20k sources: $(du -sh "$OUT" --exclude=target | cut -f1)  target: $(du -sh "$OUT/target" 2>/dev/null | cut -f1)"
  df -h / | tail -1
  EV="$REPO/scripts/evidence/g2_1_split"
  cp "$WORK"/build*.time "$WORK/javac.time" "$EV/" 2>/dev/null
  cp "$WORK/rust.out" "$EV/farm20k_native_stdout.txt"
  cp "$WORK/java.out" "$EV/farm20k_jvm_stdout.txt"
  cp "$LOG" "$EV/farm20k_remeasure.log"
  say "evidence copied to $EV"
  say "================ G2.1 20k RE-MEASURE END"
  ;;
*)
  say "unknown stage $1"; exit 2
  ;;
esac
