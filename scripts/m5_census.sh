#!/bin/bash
# M5 T01 scale census (docs/M5_PLAN.md §2): generate the farm, javac it,
# then time the j2r stages separately. Usage: m5_census.sh N [workdir]
# Stages timed via scripts/timeit.py (WALL_S PEAK_RSS_KB; /usr/bin/time
# is absent in the sandbox, getrusage(RUSAGE_CHILDREN) is equivalent).
set -euo pipefail
cd "$(dirname "$0")/.."
N="${1:?usage: m5_census.sh N [workdir]}"
WORK="${2:-/tmp/m5_farm}"
export PATH="$HOME/.cargo/bin:/home/z/tools/jdk-21.0.13+9/bin:$PATH"
export JAVA_HOME=/home/z/tools/jdk-21.0.13+9
J2R="$PWD/target/release/j2r"
CSV="scripts/m5_census.csv"
mkdir -p "$WORK"
ROW="$N"

echo "== [1/4] generate $N classes"
python3 scripts/m5_farm_gen.py "$N" "$WORK"

echo "== [2/4] javac"
cd "$WORK/farm_src"
if [ ! -d classes ]; then
  python3 "$OLDPWD/scripts/timeit.py" -o javac.time -- \
    javac -d classes @sources.txt 2> javac.err
fi
T=$(cut -d' ' -f1 javac.time); M=$(cut -d' ' -f2 javac.time)
ROW="$ROW,$T,$M"
cd "$OLDPWD"

echo "== [3/4] jar"
JAR="$WORK/farm_$N.jar"
if [ ! -f "$JAR" ]; then
  (cd "$WORK/farm_src/classes" && jar cf "$JAR" farm)
fi

echo "== [4/4] j2r stages"
# stage A: compile (closure + parse + IR + emit), no cargo
python3 scripts/timeit.py -o "$WORK/compile.time" -- \
  "$J2R" compile --classpath "$JAR" --main-class farm.Main \
  --crate-name "farm$N" --out "$WORK/out$N" 2> "$WORK/compile.err" || {
    echo "COMPILE FAILED:"; cat "$WORK/compile.err"; exit 1; }
T=$(cut -d' ' -f1 "$WORK/compile.time"); M=$(cut -d' ' -f2 "$WORK/compile.time")
ROW="$ROW,$T,$M"

# stage B: cargo debug build of the emitted crate (known-SIGKILL risk at scale)
export RUST_MIN_STACK=33554432
if python3 scripts/timeit.py -o "$WORK/cargo.time" -- \
  cargo build --manifest-path "$WORK/out$N/Cargo.toml" --quiet \
  > "$WORK/cargo.log" 2>&1; then
  T=$(cut -d' ' -f1 "$WORK/cargo.time"); M=$(cut -d' ' -f2 "$WORK/cargo.time")
  ROW="$ROW,$T,$M"
else
  ROW="$ROW,FAIL,FAIL"
  echo "cargo build failed (see $WORK/cargo.log tail):"; tail -5 "$WORK/cargo.log"
fi

HDR="n,javac_s,javac_kb,compile_s,compile_kb,cargo_s,cargo_kb"
printf '%s\n' "$ROW"
if [ ! -f "$CSV" ]; then printf '%s\n' "$HDR" > "$CSV"; fi
printf '%s\n' "$ROW" >> "$CSV"
