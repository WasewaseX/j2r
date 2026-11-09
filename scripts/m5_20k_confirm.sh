#!/bin/bash
# M5 wave-1 closer — TODO.md §M5 "[~] ... remaining: 20k census-jar
# confirmation of the same three numbers" (cold / no-op / one-body-edit),
# extended with the wave-1 acceptance bars from M5_PLAN §2:
#   - 20k farm compiles + builds end-to-end, wall + peak RSS recorded
#   - 1k -> 20k stage curve checked for super-linearity against the census
#   - a no-change rebuild does measurably less work than the first
#   - output parity vs Temurin on stdout/stderr/exit
# Profile: debug (the census profile), cargo -j 1 so RUSAGE_CHILDREN peak
# RSS stays a single-process number (same semantics as T01).
# Disk note: CARGO_INCREMENTAL=0 and no --release leg on this 6.6 GB box;
# the release-profile 20k trio stays registered as follow-up.
set -uo pipefail
source /home/z/my-project/env.sh
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=33554432

REPO=/home/z/my-project/work/j2r-gh
WORK=/home/z/my-project/work/m5invest/farm20k
N=20000
K=250
LOG="$WORK/confirm.log"
mkdir -p "$WORK"
exec >> "$LOG" 2>&1
say() { echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*"; }

say "================ 20k confirmation START (N=$N k=$K debug -j1 CI=0)"
df -h / | tail -1

say "== [0/5] build the compiler (release)"
if ! (cd "$REPO" && cargo build --release --quiet -j 2); then
  say "COMPILER BUILD FAILED"; exit 1
fi
J2R="$REPO/target/release/j2r"
say "compiler ready: $J2R"

say "== [1/5] generate $N-class farm + javac + jar"
if [ ! -f "$WORK/farm_src/sources.txt" ]; then
  python3 "$REPO/scripts/m5_farm_gen.py" "$N" "$WORK" || { say "GEN FAILED"; exit 1; }
fi
cd "$WORK/farm_src"
if [ ! -d classes ]; then
  python3 "$REPO/scripts/timeit.py" -o "$WORK/javac.time" -- \
    javac -d classes @sources.txt 2> javac.err || { say "JAVAC FAILED"; tail -3 javac.err; exit 1; }
  say "javac WALL_S PEAK_RSS_KB = $(cat "$WORK/javac.time")"
fi
if [ ! -f "$WORK/farm$N.jar" ]; then
  (cd "$WORK/farm_src/classes" && jar cf "$WORK/farm$N.jar" farm)
fi
cd "$REPO"

run_build() { # $1 = phase tag (build{1,2,3}), runs compile leg then cargo leg
  local tag="$1"
  python3 "$REPO/scripts/timeit.py" -o "$WORK/$tag.compile.time" -- \
    "$J2R" compile --classpath "$WORK/farm$N.jar" --main-class farm.Main \
    --crate-name farm20000 --out "$WORK/out20k" --partition-classes "$K" \
    > "$WORK/$tag.compile.log" 2>&1 || { say "$tag COMPILE FAILED"; tail -5 "$WORK/$tag.compile.log"; exit 1; }
  say "$tag compile (emit incl. manifest) WALL_S PEAK_RSS_KB = $(cat "$WORK/$tag.compile.time")"
  grep -E "manifest:" "$WORK/$tag.compile.log" | tail -1 || true
  python3 "$REPO/scripts/timeit.py" -o "$WORK/$tag.cargo.time" -- \
    cargo build --manifest-path "$WORK/out20k/Cargo.toml" --quiet -j 1 \
    > "$WORK/$tag.cargo.log" 2>&1 || { say "$tag CARGO FAILED"; tail -8 "$WORK/$tag.cargo.log"; exit 1; }
  say "$tag cargo debug -j1 WALL_S PEAK_RSS_KB = $(cat "$WORK/$tag.cargo.time")"
}

say "== [2/5] COLD build (fresh out dir)"
rm -rf "$WORK/out20k"
run_build build1

say "== [3/5] NO-OP rebuild (nothing changed)"
run_build build2

say "== [4/5] ONE-CLASS BODY EDIT (C00042 constant tweaked, same signature)"
python3 - <<'EOF'
p = '/home/z/my-project/work/m5invest/farm20k/farm_src/farm/C00042.java'
src = open(p).read()
src2 = src.replace('seed + i *', 'seed + i * 2 *', 1)
assert src2 != src, "tweak must change the file"
open(p, 'w').write(src2)
EOF
javac -d "$WORK/farm_src/classes" -cp "$WORK/farm_src/classes" \
  "$WORK/farm_src/farm/C00042.java" || { say "EDIT JAVAC FAILED"; exit 1; }
jar cf "$WORK/farm$N.jar" -C "$WORK/farm_src/classes" .
run_build build3

say "== [5/5] parity vs Temurin + footprint"
BIN=$(find "$WORK/out20k/target/debug" -maxdepth 1 -type f -perm -111 \
      \( -name "farm20000" -o -name "farm_20000" \) | head -1)
say "binary: $BIN"
"$BIN" > "$WORK/rust.out" 2> "$WORK/rust.err"; RC=$?
java -cp "$WORK/farm_src/classes" farm.Main > "$WORK/java.out" 2> "$WORK/java.err"; RJ=$?
say "exit codes: rust=$RC java=$RJ"
if diff -q "$WORK/rust.out" "$WORK/java.out" >/dev/null \
   && diff -q "$WORK/rust.err" "$WORK/java.err" >/dev/null \
   && [ "$RC" = "$RJ" ]; then
  say "PARITY: PASS (stdout+stderr+exit identical)"
  echo "PASS" > "$WORK/PARITY"
else
  say "PARITY: FAIL — see $WORK/rust.out vs $WORK/java.out (head diff below)"
  diff "$WORK/java.out" "$WORK/rust.out" | head -5 || true
  echo "FAIL" > "$WORK/PARITY"
fi
say "out20k: $(du -sh "$WORK/out20k" | cut -f1)  target: $(du -sh "$WORK/out20k/target" 2>/dev/null | cut -f1)"
df -h / | tail -1
say "================ 20k confirmation END"
