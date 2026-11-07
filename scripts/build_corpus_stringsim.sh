#!/bin/bash
# Build the java-string-similarity real-world corpus file: one .java,
# multiple package-private classes + a public StringsSimCorpus driver.
# Source: tdebatty/java-string-similarity v0.24 (Apache-2.0), vendored to
# corpus/stringsim/ — the same library an external regex-shaped converter
# mangled into 309 rustc errors; through j2r it must be byte-identical to
# Temurin on first differential contact.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=corpus/StringsSimCorpus.java
IMP=$(mktemp)
FRAG=$(mktemp)
DRV=$(mktemp)
trap 'rm -f "$IMP" "$FRAG" "$DRV"' EXIT

cat > "$DRV" <<'DRIVER'
/**
 * Differential driver over the java-string-similarity library
 * (real-world corpus entry 4). Deterministic output only: every double is
 * printed via string concatenation (the shortest-repr law), profiles are
 * printed by entrySet iteration (the HashMap order law), and the inputs
 * pin the three external-report silent-semantic classes: UTF-16 code units
 * (astral = 2 units), f32 intermediates (JaroWinkler bits), and the ASCII
 * \s law (NBSP must survive whitespace collapse).
 */
public class StringsSimCorpus {

  // Named impl instead of an anonymous class: deterministic, and the
  // interface dispatch through a user implementation is the row that
  // matters (vtable through CharacterSubstitutionInterface).
  static class DoubleSub implements CharacterSubstitutionInterface {
    public double cost(char c1, char c2) {
      if (c1 == c2) return 0.0;
      return 1.0;
    }
  }

  static class HalfSub implements CharacterSubstitutionInterface {
    public double cost(char c1, char c2) {
      if (c1 == c2) return 0.0;
      return 0.5;
    }
  }

  static void pp(String label, java.util.Map<String, Integer> profile) {
    StringBuilder sb = new StringBuilder();
    for (java.util.Map.Entry<String, Integer> e : profile.entrySet()) {
      if (sb.length() > 0) sb.append(' ');
      sb.append(e.getKey()).append('=').append(e.getValue());
    }
    System.out.println(label + " {" + sb.toString() + "}");
  }

  static void ppk(String label, java.util.Map<Character, Integer> profile) {
    StringBuilder sb = new StringBuilder();
    for (java.util.Map.Entry<Character, Integer> e : profile.entrySet()) {
      if (sb.length() > 0) sb.append(' ');
      sb.append(e.getKey()).append('=').append(e.getValue());
    }
    System.out.println(label + " {" + sb.toString() + "}");
  }

  public static void main(String[] args) {
    String a = "My String";
    String b = "My String2";
    String emoji = "\uD83D\uDC4D";
    String x = "x";

    // --- Levenshtein: the UTF-16 law row + nulls + the 3-arg overload ---
    Levenshtein lev = new Levenshtein();
    System.out.println("lev_ab=" + lev.distance(a, b));
    System.out.println("lev_emoji_x=" + lev.distance(emoji, x));
    System.out.println("lev_empty=" + lev.distance("", "abc"));
    try {
      lev.distance(null, b);
    } catch (NullPointerException e) {
      System.out.println("lev_null: " + e.getMessage());
    }

    // --- NormalizedLevenshtein ------------------------------------------
    NormalizedLevenshtein nl = new NormalizedLevenshtein();
    System.out.println("nl_d=" + nl.distance(a, b)
        + " nl_s=" + nl.similarity(a, b));
    System.out.println("nl_emoji=" + nl.distance(emoji, x));

    // --- MetricLCS + LCS -------------------------------------------------
    MetricLCS mlcs = new MetricLCS();
    System.out.println("mlcs=" + mlcs.distance(a, b));
    LongestCommonSubsequence lcs = new LongestCommonSubsequence();
    System.out.println("lcs=" + lcs.distance(a, b));

    // --- Damerau + OSA ---------------------------------------------------
    Damerau dam = new Damerau();
    System.out.println("dam_ab=" + dam.distance(a, b));
    System.out.println("dam_swap=" + dam.distance("abcd", "abdc"));
    OptimalStringAlignment osa = new OptimalStringAlignment();
    System.out.println("osa=" + osa.distance("abcd", "abdc"));

    // --- JaroWinkler: the f32-fidelity row (B17) -------------------------
    JaroWinkler jw = new JaroWinkler();
    System.out.println("jw=" + jw.similarity(a, b));
    System.out.println("jw_emoji=" + jw.similarity(emoji, x));

    // --- NGram: the float[] DP law ---------------------------------------
    NGram ng = new NGram();
    System.out.println("ng=" + ng.distance(a, b));
    NGram ng4 = new NGram(4);
    System.out.println("ng4=" + ng4.distance(a, b));

    // --- ShingleBased profile + the ASCII \s law (B21) -------------------
    QGram q2 = new QGram(2);
    pp("q2_profile_ws", q2.getProfile("a  b\n\tc"));
    // NBSP (\u00A0) is NOT Java \s — the profile must keep it verbatim.
    pp("q2_profile_nbsp", q2.getProfile("a\u00A0b"));
    System.out.println("q2=" + q2.distance(a, b));
    QGram qd = new QGram();
    System.out.println("q2_default_k=" + qd.distance(a, b));

    // --- Jaccard (extends ShingleBased, HashMap profile) -----------------
    Jaccard jac = new Jaccard(3);
    System.out.println("jac=" + jac.similarity(a, b));
    System.out.println("jac_ws=" + jac.similarity("a  b", "a b"));
    System.out.println("jac_nbsp=" + jac.similarity("a\u00A0b", "a b"));

    // --- SorensenDice ----------------------------------------------------
    SorensenDice sd = new SorensenDice();
    System.out.println("sd=" + sd.similarity(a, b));
    System.out.println("sd_emoji=" + sd.similarity(emoji, x + x));

    // --- Cosine: entrySet-driven aggregation over profiles ---------------
    Cosine cos = new Cosine(3);
    System.out.println("cos=" + cos.similarity(a, b));
    System.out.println("cos_emoji=" + cos.similarity(emoji, x));

    // --- WeightedLevenshtein: interface dispatch -------------------------
    WeightedLevenshtein wl = new WeightedLevenshtein(new DoubleSub());
    System.out.println("wl=" + wl.distance(a, b));
    WeightedLevenshtein wl2 = new WeightedLevenshtein(new HalfSub());
    System.out.println("wl_half=" + wl2.distance("abcd", "abce"));
    try {
      wl2.distance(null, b);
    } catch (NullPointerException e) {
      System.out.println("wl_null: " + e.getMessage());
    }

    // --- Sift4 (experimental): LinkedList + local class ------------------
    Sift4 sift = new Sift4();
    System.out.println("sift_ab=" + sift.distance(a, b));
    System.out.println("sift_emoji=" + sift.distance(emoji, x));
    System.out.println("sift_trans=" + sift.distance("abcd", "abdc"));

    // --- error paths ------------------------------------------------------
    try {
      new QGram(0);
    } catch (IllegalArgumentException e) {
      System.out.println("qgram0: " + e.getMessage());
    }
    try {
      new NGram(-1);
    } catch (IllegalArgumentException e) {
      System.out.println("ngram_neg: " + e.getMessage());
    }
    System.out.println("empty_profile=" + new QGram(2).getProfile("").size());
  }
}
DRIVER

# Library files: drop package + intra-library imports; demote top-level
# public; split imports from bodies.
for f in corpus/stringsim/CharacterSubstitutionInterface.java \
         corpus/stringsim/Cosine.java corpus/stringsim/Damerau.java \
         corpus/stringsim/Jaccard.java corpus/stringsim/JaroWinkler.java \
         corpus/stringsim/Levenshtein.java \
         corpus/stringsim/LongestCommonSubsequence.java \
         corpus/stringsim/MetricLCS.java corpus/stringsim/NGram.java \
         corpus/stringsim/NormalizedLevenshtein.java \
         corpus/stringsim/OptimalStringAlignment.java \
         corpus/stringsim/QGram.java corpus/stringsim/ShingleBased.java \
         corpus/stringsim/SorensenDice.java \
         corpus/stringsim/WeightedLevenshtein.java \
         corpus/stringsim/experimental/Sift4.java \
         corpus/stringsim/interfaces/MetricStringDistance.java \
         corpus/stringsim/interfaces/NormalizedStringDistance.java \
         corpus/stringsim/interfaces/NormalizedStringSimilarity.java \
         corpus/stringsim/interfaces/StringDistance.java \
         corpus/stringsim/interfaces/StringSimilarity.java; do
  # @Immutable (net.jcip) is third-party metadata with zero semantics —
  # stripped alongside the package/imports, like the JAMA wave's merge.
  grep -v -e '^import info\.debatty\.java\.stringsimilarity\.' \
          -e '^import net\.jcip\.annotations\.' \
          -e '^@Immutable' "$f"
done | sed -e 's/^package .*;//' \
    -e 's/^public \(final \|abstract \)\+\(class\|interface\|enum\)/\1\2/' \
    -e 's/^public \(class\|interface\|enum\)/\1/' | while IFS= read -r line; do
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

javac -d /tmp/stringsim_corpus_out "$OUT" && echo JAVAC_OK
