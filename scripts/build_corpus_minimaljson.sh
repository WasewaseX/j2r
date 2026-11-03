#!/bin/bash
# Build the minimal-json real-world corpus file: one .java, multiple
# package-private classes + a public MinimalJsonCorpus driver entry point.
# Source: ralfstx/minimal-json (Apache-2.0), fetched to corpus/minimal-json/.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=corpus/MinimalJsonCorpus.java
IMP=$(mktemp)
FRAG=$(mktemp)
DRV=$(mktemp)
trap 'rm -f "$IMP" "$FRAG" "$DRV"' EXIT

cat > "$DRV" <<'DRIVER'
/**
 * Differential driver over the minimal-json library (real-world corpus).
 * Deterministic output only; every println is stable across JVMs.
 */
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

# Library files: drop package + old-package imports; split imports from bodies.
for f in corpus/minimal-json/*.java; do
  grep -v '^import com\.eclipsesource\.json\.' "$f"
done | sed -e 's/^package .*;//' \
    -e 's/^public \(final \|abstract \)\+\(class\|interface\|enum\)/\1\2/' \
    -e 's/^public \(class\|interface\|enum\)/\1/' \
    -e 's/Iterable<Member>/Iterable<JsonObject.Member>/' | while IFS= read -r line; do
  case "$line" in
    import\ *) printf '%s\n' "$line" >> "$IMP" ;;
    *) printf '%s\n' "$line" >> "$FRAG" ;;
  esac
done

{
  sort -u "$IMP"
  cat "$DRV"
  cat "$FRAG"
} > "$OUT"

javac -d /tmp/mj_corpus_out "$OUT" && echo JAVAC_OK
