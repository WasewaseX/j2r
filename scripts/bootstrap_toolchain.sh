#!/usr/bin/env bash
# j2r toolchain bootstrap — survives sandbox resets.
# Byte-exact pins proven during the reset #21/#22 recoveries (2026-11-13/14):
#   rustup-init  21,347,088 B   https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
#   Temurin JDK  207,858,352 B  jdk-21.0.13+9 (OpenJDK21U-jdk_x64_linux_hotspot_21.0.13_9.tar.gz)
# Toolchain pinned: rustc 1.99.1 (the stable the boards were proven on), javac 21.0.13+9.
# Lesson from reset #21: never trust skip-if-exists — VERIFY the binaries actually run.
set -euo pipefail

ROOT=/home/z/toolchain
DL=$ROOT/downloads
mkdir -p "$DL"
export RUSTUP_HOME=$ROOT/rustup
export CARGO_HOME=$ROOT/cargo

# --- rust ------------------------------------------------------------------
RUSTUP_INIT=$DL/rustup-init
if [ ! -f "$RUSTUP_INIT" ] || [ "$(stat -c%s "$RUSTUP_INIT")" != "21347088" ]; then
  echo "downloading rustup-init ..."
  curl -fsSL -o "$RUSTUP_INIT" https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
fi
[ "$(stat -c%s "$RUSTUP_INIT")" = "21347088" ] || { echo "FATAL: rustup-init size mismatch: $(stat -c%s "$RUSTUP_INIT")"; exit 1; }

if [ -x "$ROOT/cargo/bin/rustc" ] && "$ROOT/cargo/bin/rustc" --version 2>/dev/null | grep -q 'rustc 1.99.1'; then
  echo "rust 1.99.1 already present (verified)"
else
  echo "installing rust 1.99.1 ..."
  chmod +x "$RUSTUP_INIT"
  RUSTUP_HOME=$ROOT/rustup CARGO_HOME=$ROOT/cargo "$RUSTUP_INIT" -y --no-modify-path --default-toolchain 1.99.1 >/dev/null
fi
"$ROOT/cargo/bin/rustc" --version
"$ROOT/cargo/bin/cargo" --version

# --- jdk -------------------------------------------------------------------
JDK_TGZ=$DL/OpenJDK21U-jdk_x64_linux_hotspot_21.0.13_9.tar.gz
if [ ! -f "$JDK_TGZ" ] || [ "$(stat -c%s "$JDK_TGZ")" != "207858352" ]; then
  echo "downloading Temurin 21.0.13+9 tarball ..."
  curl -fsSL -o "$JDK_TGZ" https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jdk_x64_linux_hotspot_21.0.13_9.tar.gz
fi
[ "$(stat -c%s "$JDK_TGZ")" = "207858352" ] || { echo "FATAL: Temurin tarball size mismatch: $(stat -c%s "$JDK_TGZ")"; exit 1; }

if [ -x "$ROOT/jdk/bin/javac" ] && "$ROOT/jdk/bin/javac" -version 2>&1 | grep -q '21.0.13'; then
  echo "javac 21.0.13+9 already present (verified)"
else
  echo "unpacking Temurin ..."
  rm -rf "$ROOT/jdk-tmp" && mkdir -p "$ROOT/jdk-tmp"
  tar -xzf "$JDK_TGZ" -C "$ROOT/jdk-tmp" --strip-components=1
  rm -rf "$ROOT/jdk" && mv "$ROOT/jdk-tmp" "$ROOT/jdk"
fi
"$ROOT/jdk/bin/javac" -version

echo "export RUSTUP_HOME=$ROOT/rustup" > "$ROOT/env.sh"
echo "export CARGO_HOME=$ROOT/cargo" >> "$ROOT/env.sh"
echo "export PATH=\"$ROOT/cargo/bin:$ROOT/jdk/bin:\$PATH\"" >> "$ROOT/env.sh"
echo "BOOTSTRAP-OK"
