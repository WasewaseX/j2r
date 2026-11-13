#!/usr/bin/env bash
# G3.3 slice 1 — the signature-pin evidence run (real javac round-trip).
#
# 1. Compiles scripts/evidence/g3_3/SigProbe.java with the pinned
#    Temurin javac 21.0.13+9.
# 2. Records javap -v's attribute dump for the class (the raw javac
#    evidence: Signature on the field/method/class, LocalVariableTypeTable
#    inside Code).
# 3. Runs the env-gated integration test with J2R_SIG_PROBE_CLASS set —
#    j2r parses the real classfile and extracts the G3.3 map pins.
# Output: this script's own stdout, captured as
# scripts/evidence/g3_3/signature_pin_2026-11-13.txt
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
cd "$REPO"
# shellcheck disable=SC1091
source /home/z/my-project/env.sh

echo "== G3.3 slice 1: the signature-pin evidence run =="
echo "date: $(date -u +'%Y-%m-%d %H:%M:%S UTC')"
echo "javac: $("$J2R_JAVAC" -version 2>&1)"
echo

WORK="$HERE/.probe_work"
rm -rf "$WORK"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

echo '== step 1: javac compiles SigProbe.java — DEFAULT flags AND -g =='
echo '(the recorded law: default javac -g:source,lines emits Signature'
echo ' attributes but NO LocalVariableTypeTable; -g adds the LVTT)'
"$J2R_JAVAC" -d "$WORK/default" "$HERE/SigProbe.java"
"$J2R_JAVAC" -g -d "$WORK/g" "$HERE/SigProbe.java"
ls -la "$WORK/default/SigProbe.class" "$WORK/g/SigProbe.class"
echo

echo '== step 2: javap -v — the attributes javac 21 actually emits (-g build) =='
# (head closing the pipe early trips pipefail — keep the capture explicit)
JAVAP_OUT="$("$JAVA_HOME/bin/javap" -v -p -cp "$WORK/g" SigProbe)"
echo "$JAVAP_OUT" | grep -E 'Signature:|LocalVariableTypeTable|descriptor: Ljava/util/Map|static java.util.Map' | head -30
echo

echo '== step 3: the env-gated j2r parse + pin extraction (both builds) =='
export J2R_SIG_PROBE_DIR="$WORK"
cargo test -p j2r-codegen --test signature_probe -- --nocapture 2>&1 \
  | sed -n '/running/,$p'
