#!/bin/bash
# Build the JZlib real-world corpus file: one .java, multiple
# package-private classes + a public JZlibCorpus driver entry point.
# Source: ymnk/jzlib (BSD-style license, corpus/jzlib/LICENSE.txt),
# vendored to corpus/jzlib/ (23 files, ~7.2k lines).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=corpus/JZlibCorpus.java
IMP=$(mktemp)
FRAG=$(mktemp)
DRV=$(mktemp)
trap 'rm -f "$IMP" "$FRAG" "$DRV"' EXIT

cat > "$DRV" <<'DRIVER'
/**
 * Differential driver over the JZlib library (real-world corpus, entry 2).
 * Deterministic output only; every println is stable across JVMs.
 * Core deflate/inflate through the public ZStream API (levels 0-9
 * sampled, Z_FINISH/Z_NO_FLUSH semantics, wrapper off), Adler32 and
 * CRC32 checksums over a fixed vector, and the library version string.
 */
public class JZlibCorpus {
  public static void main(String[] args) throws Exception {
    String text = "the quick brown fox jumps over the lazy dog; repeat, repeat, repeat; 0123456789";
    byte[] data = text.getBytes("UTF-8");
    for (int level = 0; level <= 9; level += 3) {
      ZStream z = new ZStream();
      z.deflateInit(level);
      z.next_in = data; z.next_in_index = 0; z.avail_in = data.length;
      byte[] comp = new byte[256];
      z.next_out = comp; z.next_out_index = 0; z.avail_out = comp.length;
      int dret = z.deflate(JZlib.Z_FINISH);
      int n = z.next_out_index;
      z.deflateEnd();
      Inflater inf = new Inflater();
      inf.inflateInit();
      inf.next_in = comp; inf.next_in_index = 0; inf.avail_in = n;
      byte[] back = new byte[256];
      inf.next_out = back; inf.next_out_index = 0; inf.avail_out = back.length;
      int ret;
      do { ret = inf.inflate(JZlib.Z_NO_FLUSH); } while (ret == JZlib.Z_OK && inf.avail_in > 0);
      int m = inf.next_out_index;
      inf.inflateEnd();
      String round = new String(back, 0, m, "UTF-8");
      System.out.println("level=" + level + " dret=" + dret + " comp=" + n + " iret=" + ret
          + " back=" + m + " ok=" + round.equals(text));
    }
    Adler32 a = new Adler32();
    a.update(data, 0, data.length);
    System.out.println("adler=" + Long.toHexString(a.getValue()));
    CRC32 c = new CRC32();
    c.update(data, 0, data.length);
    System.out.println("crc=" + Long.toHexString(c.getValue()));
    System.out.println("JZlib=" + JZlib.version());
  }
}
DRIVER

# Library files: drop package decls; strip top-level public modifiers
# (one default-package file, package-private classes); split imports.
for f in corpus/jzlib/*.java; do
  grep -v '^package ' "$f"
done | sed -e 's/^public \(final \|abstract \|static \)\+\(class\|interface\|enum\)/\1\2/' \
    -e 's/^final public \(class\|interface\|enum\)/final \1/' \
    -e 's/^abstract public \(class\|interface\|enum\)/abstract \1/' \
    -e 's/^public \(class\|interface\|enum\)/\1/' \
    -e '/^public[[:space:]]*$/d' | while IFS= read -r line; do
  case "$line" in
    import\ *) printf '%s\n' "$line" >> "$IMP" ;;
    *) printf '%s\n' "$line" >> "$FRAG" ;;
  esac
done

{
  sort -u "$IMP"
  cat "$FRAG"
  cat "$DRV"
} > "$OUT"

javac -d /tmp/jzlib-corpus-bin "$OUT" && echo "JAVAC_OK: $OUT"
