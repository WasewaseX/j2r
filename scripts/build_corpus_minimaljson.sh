#!/bin/bash
# Build the minimal-json corpus ENTRY in the post-amalgam layout (scanaislop
# round-4 wave: the amalgam shape retires, the vendored ralfstx/minimal-json
# tree leaves the repo as a byte-frozen evidence archive).
#
# Layout owned by this script:
#   corpus/MinimalJsonCorpus.java  — driver only (imports
#                                    minimaljsonentry.*), body unchanged
#   corpus/minimaljsonentry/       — package minimaljsonentry: 11 verbatim
#                                    per-class sources generated from the
#                                    archive (package swap only) + 9 HAND
#                                    maintained reshaped files (the four
#                                    oversized upstream classes JsonObject
#                                    934 / JsonParser 511 / JsonArray 511 /
#                                    JsonValue 495 split into <=441-line
#                                    families with receiver-static helpers;
#                                    existence check, never overwritten)
#   scripts/evidence/vendor_minimaljson.tar.gz — pristine vendor bytes
#
# REGEN-BYTE-IDENTICAL: re-running this script leaves the tree unchanged.
set -euo pipefail
cd "$(dirname "$0")/.."
ARCHIVE=scripts/evidence/vendor_minimaljson.tar.gz
ENTRY=corpus/minimaljsonentry
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$ENTRY"
tar xzf "$ARCHIVE" -C "$WORK"
VDIR="$WORK/corpus/minimal-json"

# VERBATIM generation: package swap + intra-package import strip; public
# stays public (the amalgam-only public demotion cannot cross a package
# boundary — and the SAME lesson makes the package-private upstream
# JsonNumber visible to the default-package driver: its class AND ctor
# become public). Javadoc and bytes otherwise untouched.
for f in Json JsonHandler JsonLiteral JsonNumber JsonString \
         WriterConfig PrettyPrint JsonWriter WritingBuffer \
         ParseException Location; do
  if [ "$f" = "JsonNumber" ]; then
    sed -e 's/^package com\.eclipsesource\.json;/package minimaljsonentry;/' \
        -e '/^import com\.eclipsesource\.json\./d' \
        -e 's/^class JsonNumber extends JsonValue {/public class JsonNumber extends JsonValue {/' \
        -e 's/^  JsonNumber(String string) {/  public JsonNumber(String string) {/' \
        "$VDIR/$f.java" > "$ENTRY/$f.java"
  else
    sed -e 's/^package com\.eclipsesource\.json;/package minimaljsonentry;/' \
        -e '/^import com\.eclipsesource\.json\./d' \
        "$VDIR/$f.java" > "$ENTRY/$f.java"
  fi
done

# HAND-maintained reshaped files: existence check, never overwritten.
for h in JsonObject JsonObjectMutation JsonObjectAccess HashIndexTable \
         JsonParser JsonParserTokens JsonArray JsonArrayAccess \
         JsonValue JsonValueAccess; do
  if [ ! -f "$ENTRY/$h.java" ]; then
    echo "FATAL: HAND file missing: $ENTRY/$h.java (it carries the" \
         "round-4 reshape and is never regenerated)" >&2
    exit 1
  fi
done

cat > corpus/MinimalJsonCorpus.java <<'DRIVER'
/**
 * Differential driver over the minimal-json library (real-world corpus).
 * Deterministic output only; every println is stable across JVMs.
 *
 * scanaislop round-4 wave: the driver imports the minimaljsonentry entry
 * package (the amalgam shape retires); the driver body is unchanged.
 */
import minimaljsonentry.*;

public class MinimalJsonCorpus {
  public static void main(String[] args) {
    String src = "{\"name\":\"j2r\",\"wave\":3,\"tags\":[\"a\",\"b\\\"c\"],"
        + "\"nested\":{\"pi\":3.14,\"on\":true,\"off\":false,\"nil\":null},\"empty\":{}}";
    JsonValue v = Json.parse(src);
    System.out.println(v.toString(WriterConfig.PRETTY_PRINT));
    JsonObject o = new JsonObject();
    o.add("k", 42).add("s", "esc\"\t\n<>").add("arr", new JsonArray().add(1).add(2.5).add(false));
    System.out.println(o.toString());
    System.out.println("roundtrip=" + Json.parse(o.toString()).toString().equals(o.toString()));
    System.out.println("name=" + v.asObject().get("name").asString());
    System.out.println("pi=" + v.asObject().get("nested").asObject().get("pi").asDouble());
    System.out.println("tags=" + v.asObject().get("tags").asArray().size());
    try {
      Json.parse("{bad}");
    } catch (ParseException e) {
      System.out.println("err@" + e.getLocation().line + ":" + e.getLocation().column
          + " " + e.getMessage());
    }
    System.out.println(new JsonNumber("12345678901234567890").toString());
    System.out.println("literal=" + Json.value(true).toString() + "," + Json.value(null).toString());
    JsonArray nums = new JsonArray().add(0).add(-1).add(0.5).add(1e10);
    System.out.println("sum=" + (nums.get(0).asInt() + nums.get(1).asInt())
        + " half=" + nums.get(2).asDouble() + " big=" + nums.get(3).toString());
  }
}
DRIVER

javac -sourcepath corpus -d /tmp/mj_corpus_out corpus/MinimalJsonCorpus.java && echo JAVAC_OK
