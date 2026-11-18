/**
 * Steady-state bench driver over the java-string-similarity library
 * (same entry package as StringsSimCorpus). Deterministic output only:
 * every double is printed via string concatenation (the shortest-repr
 * law), checksums accumulate in pair order, and the W7 profile rows
 * deliberately put float aggregation under HashMap iteration (the
 * OpenJDK-hash-order law) — j2r must reproduce the JVM's bits, not a
 * reassociated sum. Sized so the full-tiered JVM run lands ~2 s on the
 * 2-core reference sandbox; the native channel is measured with
 * --native-reps 1 --native-warmups 0 per the JamaBench protocol.
 *
 * scanaislop round-4 wave: the driver imports stringsimentry.* (the
 * amalgam shape retires); the driver body is unchanged.
 */
import stringsimentry.*;

public class StringsSimBench {

  static final String LOW26 = "abcdefghijklmnopqrstuvwxyz";
  static final String DNA = "ACGT";

  // xorshift64* — fixed seed, identical stream on every channel (the
  // same-law precedent as JamaBench's driver PRNG).
  static long xs = 0x243F6A8885A308D3L;

  static long xorshift64star() {
    long x = xs;
    x ^= x >>> 12;
    x ^= x << 25;
    x ^= x >>> 27;
    xs = x;
    return x * 2685821657736338717L;
  }

  static int rint(int bound) {
    return (int) Math.floorMod(xorshift64star(), bound);
  }

  static String rstr(int minLen, int maxLen, String alpha) {
    int len = minLen + rint(maxLen - minLen + 1);
    StringBuilder sb = new StringBuilder(len);
    for (int i = 0; i < len; i++) {
      sb.append(alpha.charAt(rint(alpha.length())));
    }
    return sb.toString();
  }

  public static void main(String[] args) {
    // --- W1: Levenshtein, short pairs (per-call allocation shape) ------
    Levenshtein lev = new Levenshtein();
    double s1 = 0.0;
    for (int i = 0; i < 200_000; i++) {
      s1 += lev.distance(rstr(8, 16, LOW26), rstr(8, 16, LOW26));
    }

    // --- W5: JaroWinkler, record-linkage shape --------------------------
    JaroWinkler jw = new JaroWinkler();
    double s5 = 0.0;
    for (int i = 0; i < 500_000; i++) {
      s5 += jw.similarity(rstr(6, 12, LOW26), rstr(6, 12, LOW26));
    }

    // --- W7a/W7b: profile build once, then the precompute-and-reuse -----
    // pattern the library documents: getProfile per document (HashMap
    // insertion order = the OpenJDK law), then cosine over PREBUILT
    // profiles — dot_product/norm iterate the profiles' entry sets, so
    // the checksum accumulates floats in HashMap iteration order.
    Cosine cos = new Cosine(3);
    String[] vocab = new String[200];
    for (int i = 0; i < 200; i++) {
      vocab[i] = rstr(5, 9, LOW26);
    }
    String[] docs = new String[1000];
    for (int i = 0; i < 1000; i++) {
      int words = 70 + rint(141);
      StringBuilder sb = new StringBuilder(words * 8);
      for (int j = 0; j < words; j++) {
        if (j > 0) sb.append(' ');
        sb.append(vocab[rint(200)]);
      }
      docs[i] = sb.toString();
    }
    // getProfile returns an unmodifiable view — the array's component
    // type must be Map, not HashMap (the store check is real).
    java.util.Map<String, Integer>[] profiles = new java.util.Map[1000];
    long sizeSum = 0;
    for (int i = 0; i < 1000; i++) {
      profiles[i] = cos.getProfile(docs[i]);
      sizeSum += profiles[i].size();
    }
    double s7 = 0.0;
    for (int i = 0; i < 10_000; i++) {
      s7 += cos.similarity(profiles[rint(1000)], profiles[rint(1000)]);
    }

    // --- W8: NGram(2) + NGram(4), f32 DP law ----------------------------
    NGram ng2 = new NGram(2);
    NGram ng4 = new NGram(4);
    double s8 = 0.0;
    for (int i = 0; i < 10_000; i++) {
      String a = rstr(40, 80, DNA);
      String b = rstr(40, 80, DNA);
      s8 += ng2.distance(a, b);
      s8 += ng4.distance(a, b);
    }

    System.out.println("w1_lev_short=" + s1);
    System.out.println("w5_jw=" + s5);
    System.out.println("w7a_profile_sizes=" + sizeSum);
    System.out.println("w7b_precomputed=" + s7);
    System.out.println("w8_ngram=" + s8);
    System.out.println("done");
  }
}
