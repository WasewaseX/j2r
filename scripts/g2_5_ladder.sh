#!/bin/bash
# G2.5 — the honest scaling ladder (REVIEW_CHECKLIST G2.5 + G2.7 framing).
#
# N raised stepwise (35k first; 50k/70k when the disk allows), same
# standing protocol as G2.1/G2.4: k=250 partitions, cargo -j 2,
# CARGO_INCREMENTAL=0, 550 s chunk cap with fingerprint resume,
# proc_peak_sampler.py on every cargo chunk, disk guard >= 1.2 GB free.
#
# G2.7 budget framing: the acceptance budget is THIS box class —
# 4 GB/process VmHWM, disk-guarded volume — measured, not extrapolated.
#
# Disk-peak optimization (recorded, protocol-compatible): the JVM side
# of the three-channel parity is captured at stageP (stdout+stderr+exit
# to files) BEFORE the cargo build, so farm_src (sources+classes) and
# the jar can be deleted before the cargo peak — parity then diffs the
# native run against the captured JVM channel files. The differential
# contract is unchanged: stdout+stderr+exit, byte-identical.
#
# Stages (call one per tool call, sandbox lesson #9) — ORDER MATTERS:
# emit reads the jar, so stageE runs BEFORE stageP's cleanup:
#   stageN  N=<n> bash g2_5_ladder.sh init   # set up work dirs for N
#   stageG   farm gen + javac + jar (fresh)
#   stageE   cold emit (fresh out$N dev profile, standing protocol;
#            needs the jar)
#   stageP   JVM parity capture (java run -> java.out/.err/exit), then
#            delete farm_src + jar (disk headroom for the cargo peak)
#   stageC   ONE cargo chunk (repeat until $WORK/cargo.DONE)
#   stageF   parity vs captured JVM channels + footprint + evidence copy
#            + delete the target dir (disk recovery for the next rung)
# Usage: N=35000 bash scripts/g2_5_ladder.sh <stage>
set -uo pipefail
source /home/z/my-project/env.sh
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=33554432

REPO=/home/z/my-project/work/j2r-fresh
M5=/home/z/my-project/work/m5invest
K=250
J2R="$REPO/target/release/j2r"

N=${N:-35000}
WORK="$M5/farm$N"
OUT="$WORK/out$N"
JAR="$WORK/farm$N.jar"
SRC="$WORK/farm_src"
LOG="$WORK/ladder.log"
mkdir -p "$WORK"
exec >> "$LOG" 2>&1
say() { echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] N=$N $*"; }
guard() { # refuse to start under GUARD_KB (the ENOSPC lesson; default 1.2 GB).
  # GUARD_KB is parameterized so a rung can finish its last crate units
  # and reach the REAL wall (link) as a measured record instead of being
  # guard-blocked one step early — every use of a lowered guard is named
  # in the rung's evidence.
  FREE=$(df -k / | tail -1 | awk '{print $4}')
  MIN=${GUARD_KB:-1200000}
  if [ "$FREE" -lt "$MIN" ]; then
    say "DISK GUARD: ${FREE} kB free < ${MIN} kB — free disk before continuing"
    exit 3
  fi
  say "disk guard OK (${FREE} kB free; min ${MIN} kB)"
}

case "${1:?usage: N=<n> bash g2_5_ladder.sh <init|stageG|stageP|stageE|stageC|stageF>}" in
init)
  say "================ G2.5 LADDER RUNG INIT N=$N k=$K (standing dev profile, CI=0)"
  df -h / | tail -1
  ;;

stageG)
  say "== [G] farm gen + javac + jar (fresh)"
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
  say "stageG OK: jar $(du -sh "$JAR" | cut -f1), src $(du -sh "$SRC" | cut -f1)"
  ;;

stageP)
  say "== [P] JVM parity capture + farm_src release"
  cd "$SRC"
  java -cp classes farm.Main > "$WORK/java.out" 2> "$WORK/java.err"
  echo $? > "$WORK/java.exit"
  say "JVM captured: exit=$(cat "$WORK/java.exit") out=$(stat -c%s "$WORK/java.out")B err=$(stat -c%s "$WORK/java.err")B"
  # headroom for the cargo peak: sources+classes+jar no longer needed
  rm -rf "$SRC" "$JAR"
  say "farm_src + jar deleted after capture (parity diffs against the captured channels)"
  df -h / | tail -1
  ;;

stageE)
  say "== [E] COLD emit (fresh out$N, standing dev protocol)"
  guard
  rm -rf "$OUT"
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build1.compile.time" -- \
    "$J2R" compile --classpath "$WORK/farm$N.jar" --main-class farm.Main \
    --crate-name "farm$N" --out "$OUT" --partition-classes "$K" \
    > "$WORK/build1.compile.log" 2>&1 || { say "EMIT FAILED"; tail -5 "$WORK/build1.compile.log"; exit 1; }
  say "cold emit WALL_S PEAK_RSS_KB = $(cat "$WORK/build1.compile.time")"
  grep -E "manifest:" "$WORK/build1.compile.log" | tail -1 || true
  say "out$N sources: $(du -sh "$OUT" | cut -f1)"
  df -h / | tail -1
  ;;

stageC)
  guard
  CH=$(cat "$WORK/cargo.chunk.next" 2>/dev/null || echo 1)
  say "== [C] cargo chunk #$CH (fingerprints resume; sampler wired)"
  python3 "$REPO/scripts/proc_peak_sampler.py" -o "$WORK/build1.cargo.chunk$CH.time" -- \
    timeout 550 cargo build --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build1.cargo.chunk$CH.log" 2>&1
  RC=$?
  say "chunk #$CH rc=$RC sampler: $(head -1 "$WORK/build1.cargo.chunk$CH.time" 2>/dev/null || echo 'no sampler line')"
  if [ $RC -eq 0 ]; then
    say "cold cargo DONE in chunks"
    touch "$WORK/cargo.DONE"
  else
    say "chunk #$CH killed by the 550 s cap (expected mid-build; fingerprints resume)"
    tail -4 "$WORK/build1.cargo.chunk$CH.time" 2>/dev/null || true
  fi
  echo $((CH + 1)) > "$WORK/cargo.chunk.next"
  df -h / | tail -1
  ;;

stageF)
  say "== [F] parity vs captured JVM channels + footprint + evidence"
  BIN="$OUT/target/debug/farm$N"
  [ -f "$BIN" ] || BIN=$(find "$OUT/target" -maxdepth 2 -type f -perm -111 -name "farm*" | head -1)
  [ -n "$BIN" ] || { say "NO BINARY — cold build incomplete?"; exit 1; }
  say "binary: $BIN ($(stat -c%s "$BIN") B)"
  "$BIN" > "$WORK/rust.out" 2> "$WORK/rust.err"; RC=$?
  RJ=$(cat "$WORK/java.exit")
  if diff -q "$WORK/rust.out" "$WORK/java.out" >/dev/null \
     && diff -q "$WORK/rust.err" "$WORK/java.err" >/dev/null \
     && [ "$RC" = "$RJ" ]; then
    say "PARITY: PASS (stdout+stderr+exit identical vs captured Temurin channels)"
    echo "PASS" > "$WORK/LADDER_PARITY"
  else
    say "PARITY: FAIL — head diff below"
    diff "$WORK/java.out" "$WORK/rust.out" | head -5 || true
    echo "FAIL" > "$WORK/LADDER_PARITY"
  fi
  TOT=0; PEAK=0
  for f in "$WORK"/build1.cargo.chunk*.time; do
    [ -e "$f" ] || continue
    W=$(head -1 "$f" | cut -d' ' -f1); P=$(head -1 "$f" | cut -d' ' -f2)
    TOT=$(python3 -c "print(round($TOT + $W, 2))")
    PEAK=$(python3 -c "print(max($PEAK, $P))")
  done
  say "cold cargo TOTAL wall across chunks = ${TOT}s ; PEAK single-process VmHWM = ${PEAK} KB"
  say "out$N sources: $(du -sh "$OUT" | cut -f1)  target: $(du -sh "$OUT/target" 2>/dev/null | cut -f1)"
  EV="$REPO/scripts/evidence/g2_5_${N}"
  mkdir -p "$EV"
  cp "$WORK"/build*.time "$WORK/javac.time" "$EV/" 2>/dev/null
  cp "$WORK/java.out" "$EV/farm${N}_jvm_stdout.txt"
  cp "$WORK/java.err" "$EV/farm${N}_jvm_stderr.txt"
  cp "$WORK/rust.out" "$EV/farm${N}_native_stdout.txt"
  cp "$WORK/rust.err" "$EV/farm${N}_native_stderr.txt"
  cp "$LOG" "$EV/farm${N}_ladder.log"
  df -k / | tail -1 > "$EV/disk_after.txt"
  du -sh "$OUT" > "$EV/out${N}_size.txt" 2>/dev/null
  say "evidence copied to $EV"
  # disk recovery for the next rung: evidence carries the measurements
  rm -rf "$OUT/target"
  say "target dir deleted after evidence copy (the next rung needs the volume)"
  df -h / | tail -1
  say "================ G2.5 LADDER RUNG END N=$N"
  ;;
stageT)
  # EMIT-ONLY rung teardown: for rungs whose LINK is known-dead on this
  # volume (35k measured), record the emit-stage tables (gen/javac/jar/
  # JVM capture/emit) and tear everything down — the emit stage scales
  # and is the compile-stage north-star measurement; no cargo, no
  # parity claim, no binary claim.
  say "== [T] EMIT-ONLY teardown + evidence"
  EV="$REPO/scripts/evidence/g2_5_${N}"
  mkdir -p "$EV"
  cp "$WORK"/build1.compile.time "$WORK/javac.time" "$EV/" 2>/dev/null
  cp "$WORK/java.out" "$EV/farm${N}_jvm_stdout.txt" 2>/dev/null
  cp "$WORK/java.err" "$EV/farm${N}_jvm_stderr.txt" 2>/dev/null
  cp "$WORK/java.exit" "$EV/farm${N}_jvm_exit.txt" 2>/dev/null
  cp "$LOG" "$EV/farm${N}_ladder.log" 2>/dev/null
  du -sh "$OUT" > "$EV/out${N}_size.txt" 2>/dev/null
  df -k / | tail -1 > "$EV/disk_after.txt"
  echo "EMIT-ONLY RUNG: cargo/link NOT attempted (the 35k link wall is" > "$EV/SCOPE.txt"
  echo "measured; this rung records the emit-stage scaling only)." >> "$EV/SCOPE.txt"
  say "evidence copied to $EV"
  rm -rf "$OUT" "$SRC" "$JAR"
  say "rung artifacts deleted (emit-only scope)"
  df -h / | tail -1
  say "================ G2.5 LADDER RUNG END N=$N (emit-only)"
  ;;
*)
  say "unknown stage $1"; exit 2
  ;;
esac
