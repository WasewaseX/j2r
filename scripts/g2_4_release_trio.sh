#!/bin/bash
# G2.4 — release-profile 20k trio (REVIEW_CHECKLIST F4).
#
# The cold/no-op/one-edit trio re-measured with the GENERATED app built
# under `cargo build --release` (opt-level 3 — the regime that killed
# JZlib pre-G2.6). Everything else identical to the standing G2.1
# protocol (scripts/g2_1_20k_remeasure.sh): N=20000, k=250 → 80 part
# crates + support + app; cargo -j 2; CARGO_INCREMENTAL=0.
#
# Sandbox lesson #9: stage-based; call one stage per tool call; cargo
# fingerprints resume across chunks; killed chunks leave the sampler's
# partial table as the honesty record (wall summed across chunks in
# stage5).
#
# G2.1b wiring (per checklist): proc_peak_sampler.py wraps every cargo
# chunk — per-process VmHWM attribution is part of the standing
# protocol from now on.
#
# Disk guard (the ENOSPC lesson): every cargo stage checks free space
# and refuses to start under MIN_FREE_KB, so the failure is a clean
# honest record, not a corrupted target.
#
# Stages:
#   stageR  rebuild the j2r compiler binary at this commit (~2 min)
#   stage0  farm gen (fresh — reset #12 wiped m5invest) + javac + jar
#   stage1  release emit, cold (fresh out20krel, fresh manifest)
#   stage2  ONE release cargo chunk of the COLD build (repeat until
#           $WORK/build1.cargo.DONE exists)
#   stage3  no-op rebuild: emit + cargo --release
#   stage4  one-body-edit (C00042) rebuild: jar + emit + cargo --release
#   stage5  parity vs Temurin + footprint + disk table + evidence copy
#
# Usage: g2_4_release_trio.sh stage<N|R>
set -uo pipefail
source /home/z/my-project/env.sh
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=33554432

REPO=/home/z/my-project/work/j2r-fresh
WORK=/home/z/my-project/work/m5invest/farm20k
N=20000
K=250
LOG="$WORK/release_trio.log"
mkdir -p "$WORK"
exec >> "$LOG" 2>&1
say() { echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*"; }
guard() { # refuse to start under MIN_FREE_KB
  FREE=$(df -k / | tail -1 | awk '{print $4}')
  if [ "$FREE" -lt 1200000 ]; then
    say "DISK GUARD: ${FREE} kB free < 1200000 kB — free disk before continuing"
    exit 3
  fi
  say "disk guard OK (${FREE} kB free)"
}

J2R="$REPO/target/release/j2r"
OUT="$WORK/out20krel"
JAR="$WORK/farm$N.jar"
SRC="$WORK/farm_src"

case "${1:?usage: g2_4_release_trio.sh stage<N|R>}" in
stageR)
  say "================ G2.4 RELEASE TRIO START (N=$N k=$K --release -j2 CI=0)"
  df -h / | tail -1
  python3 "$REPO/scripts/timeit.py" -o "$WORK/j2r_rebuild.time" -- \
    cargo build --release -p j2r-cli --quiet > "$WORK/j2r_rebuild.log" 2>&1 \
    || { say "J2R RELEASE BUILD FAILED"; tail -5 "$WORK/j2r_rebuild.log"; exit 1; }
  say "j2r release rebuild WALL_S PEAK_KB = $(cat "$WORK/j2r_rebuild.time")"
  "$J2R" --version || { say "J2R BINARY DEAD"; exit 1; }
  say "stageR OK"
  ;;

stage0)
  say "== [0] farm gen + javac + jar (fresh after reset #12)"
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
  say "stage0 OK: jar $(du -sh "$JAR" | cut -f1), src $(du -sh "$SRC" --exclude=classes | cut -f1), classes $(du -sh "$SRC/classes" | cut -f1)"
  ;;

stage1)
  say "== [1/5] COLD emit --release crate layout (fresh out20krel)"
  guard
  rm -rf "$OUT"
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build1.compile.time" -- \
    "$J2R" compile --classpath "$JAR" --main-class farm.Main \
    --crate-name farm20000 --out "$OUT" --partition-classes "$K" \
    > "$WORK/build1.compile.log" 2>&1 || { say "EMIT FAILED"; tail -5 "$WORK/build1.compile.log"; exit 1; }
  say "cold emit WALL_S PEAK_RSS_KB = $(cat "$WORK/build1.compile.time")"
  grep -E "manifest:" "$WORK/build1.compile.log" | tail -1 || true
  say "out20krel sources: $(du -sh "$OUT" --exclude=target | cut -f1)"
  ;;

stage2)
  guard
  CH=$(cat "$WORK/rel.cargo.chunk.next" 2>/dev/null || echo 1)
  say "== [2/5] COLD release cargo chunk #$CH (fingerprints resume; sampler wired)"
  python3 "$REPO/scripts/proc_peak_sampler.py" -o "$WORK/build1.cargo.chunk$CH.time" -- \
    timeout 550 cargo build --release --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build1.cargo.chunk$CH.log" 2>&1
  RC=$?
  say "chunk #$CH rc=$RC sampler: $(head -1 "$WORK/build1.cargo.chunk$CH.time" 2>/dev/null || echo 'no sampler line')"
  if [ $RC -eq 0 ]; then
    say "cold release cargo DONE in chunks (final chunk table: $(head -1 "$WORK/build1.cargo.chunk$CH.time"))"
    touch "$WORK/build1.cargo.DONE"
  else
    say "chunk #$CH killed by the 550 s cap (expected mid-build; fingerprints resume)"
    tail -4 "$WORK/build1.cargo.chunk$CH.time" 2>/dev/null || true
  fi
  echo $((CH + 1)) > "$WORK/rel.cargo.chunk.next"
  df -h / | tail -1
  ;;

stage3)
  say "== [3/5] NO-OP rebuild (nothing changed)"
  guard
  python3 "$REPO/scripts/timeit.py" -o "$WORK/build2.compile.time" -- \
    "$J2R" compile --classpath "$JAR" --main-class farm.Main \
    --crate-name farm20000 --out "$OUT" --partition-classes "$K" \
    > "$WORK/build2.compile.log" 2>&1 || { say "NOOP EMIT FAILED"; exit 1; }
  say "no-op emit WALL_S PEAK_RSS_KB = $(cat "$WORK/build2.compile.time")"
  grep -E "manifest:" "$WORK/build2.compile.log" | tail -1 || true
  python3 "$REPO/scripts/proc_peak_sampler.py" -o "$WORK/build2.cargo.time" -- \
    timeout 550 cargo build --release --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build2.cargo.log" 2>&1 || { say "NOOP CARGO FAILED"; tail -5 "$WORK/build2.cargo.log"; exit 1; }
  say "no-op release cargo WALL_S MAX_VMHWM_KB = $(cat "$WORK/build2.cargo.time")"
  ;;

stage4)
  say "== [4/5] ONE-CLASS BODY EDIT (C00042 constant tweaked, same signature)"
  guard
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
  python3 "$REPO/scripts/proc_peak_sampler.py" -o "$WORK/build3.cargo.time" -- \
    timeout 550 cargo build --release --manifest-path "$OUT/Cargo.toml" --quiet -j 2 \
    > "$WORK/build3.cargo.log" 2>&1 || { say "EDIT CARGO FAILED"; tail -5 "$WORK/build3.cargo.log"; exit 1; }
  say "one-edit release cargo WALL_S MAX_VMHWM_KB = $(cat "$WORK/build3.cargo.time")"
  ;;

stage5)
  say "== [5/5] parity vs Temurin + footprint + disk table + evidence copy"
  BIN=$(find "$OUT/target/release" -maxdepth 1 -type f -perm -111 \
        \( -name "farm20000" -o -name "farm_20000" \) | head -1)
  [ -n "$BIN" ] || { say "NO RELEASE BINARY — cold build incomplete?"; exit 1; }
  say "binary: $BIN ($(stat -c%s "$BIN") B)"
  "$BIN" > "$WORK/rust.out" 2> "$WORK/rust.err"; RC=$?
  java -cp "$SRC/classes" farm.Main > "$WORK/java.out" 2> "$WORK/java.err"; RJ=$?
  say "exit codes: rust=$RC java=$RJ"
  if diff -q "$WORK/rust.out" "$WORK/java.out" >/dev/null \
     && diff -q "$WORK/rust.err" "$WORK/java.err" >/dev/null \
     && [ "$RC" = "$RJ" ]; then
    say "PARITY: PASS (stdout+stderr+exit identical)"
    echo "PASS" > "$WORK/REL_PARITY"
  else
    say "PARITY: FAIL — head diff below"
    diff "$WORK/java.out" "$WORK/rust.out" | head -5 || true
    echo "FAIL" > "$WORK/REL_PARITY"
  fi
  TOT=0; PEAK=0
  for f in "$WORK"/build1.cargo.chunk*.time; do
    [ -e "$f" ] || continue
    W=$(head -1 "$f" | cut -d' ' -f1); P=$(head -1 "$f" | cut -d' ' -f2)
    TOT=$(python3 -c "print(round($TOT + $W, 2))")
    PEAK=$(python3 -c "print(max($PEAK, $P))")
  done
  say "cold release cargo TOTAL wall across chunks = ${TOT}s ; PEAK single-process VmHWM = ${PEAK} KB"
  say "out20krel sources: $(du -sh "$OUT" --exclude=target | cut -f1)  release target: $(du -sh "$OUT/target" 2>/dev/null | cut -f1)"
  say "emit: cold $(cat "$WORK/build1.compile.time")  no-op $(cat "$WORK/build2.compile.time")  one-edit $(cat "$WORK/build3.compile.time")"
  say "cargo release: no-op $(cat "$WORK/build2.cargo.time")  one-edit $(cat "$WORK/build3.cargo.time")"
  df -h / | tail -1
  EV="$REPO/scripts/evidence/g2_4"
  mkdir -p "$EV"
  cp "$WORK"/build*.time "$WORK/javac.time" "$WORK/j2r_rebuild.time" "$EV/" 2>/dev/null
  cp "$WORK/rust.out" "$EV/farm20k_release_native_stdout.txt"
  cp "$WORK/java.out" "$EV/farm20k_release_jvm_stdout.txt"
  cp "$LOG" "$EV/farm20k_release_trio.log"
  df -k / | tail -1 > "$EV/disk_after.txt"
  du -sh "$OUT" > "$EV/out20krel_size.txt" 2>/dev/null
  say "evidence copied to $EV"
  say "================ G2.4 RELEASE TRIO END"
  ;;
*)
  say "unknown stage $1"; exit 2
  ;;
esac
