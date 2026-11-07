import java.io.Serializable;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedList;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;
/**
 * Steady-state bench driver over the java-string-similarity library
 * (same library merge as StringsSimCorpus). Deterministic output only:
 * every double is printed via string concatenation (the shortest-repr
 * law), checksums accumulate in pair order, and the W7 profile rows
 * deliberately put float aggregation under HashMap iteration (the
 * OpenJDK-hash-order law) — j2r must reproduce the JVM's bits, not a
 * reassociated sum. Sized so the full-tiered JVM run lands ~2 s on the
 * 2-core reference sandbox; the native channel is measured with
 * --native-reps 1 --native-warmups 0 per the JamaBench protocol.
 */
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
/*
 * The MIT License
 *
 * Copyright 2015 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */



/**
 * Used to indicate the cost of character substitution.
 *
 * Cost should always be in [0.0 .. 1.0]
 * For example, in an OCR application, cost('o', 'a') could be 0.4
 * In a checkspelling application, cost('u', 'i') could be 0.4 because these are
 * next to each other on the keyboard...
 *
 * @author Thibault Debatty
 */
interface CharacterSubstitutionInterface {
    /**
     * Indicate the cost of substitution c1 and c2.
     * @param c1 The first character of the substitution.
     * @param c2 The second character of the substitution.
     * @return The cost in the range [0, 1].
     */
    double cost(char c1, char c2);
}
/*
 * The MIT License
 *
 * Copyright 2015 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */




/**
 * The similarity between the two strings is the cosine of the angle between
 * these two vectors representation. It is computed as V1 . V2 / (|V1| * |V2|)
 * The cosine distance is computed as 1 - cosine similarity.
 *
 * @author Thibault Debatty
 */
class Cosine extends ShingleBased implements
        NormalizedStringDistance, NormalizedStringSimilarity {

    /**
     * Implements Cosine Similarity between strings. The strings are first
     * transformed in vectors of occurrences of k-shingles (sequences of k
     * characters). In this n-dimensional space, the similarity between the two
     * strings is the cosine of their respective vectors.
     *
     * @param k
     */
    public Cosine(final int k) {
        super(k);
    }

    /**
     * Implements Cosine Similarity between strings. The strings are first
     * transformed in vectors of occurrences of k-shingles (sequences of k
     * characters). In this n-dimensional space, the similarity between the two
     * strings is the cosine of their respective vectors. Default k is 3.
     */
    public Cosine() {
        super();
    }

    /**
     * Compute the cosine similarity between strings.
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The cosine similarity in the range [0, 1]
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double similarity(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 1;
        }

        if (s1.length() < getK() || s2.length() < getK()) {
            return 0;
        }

        Map<String, Integer> profile1 = getProfile(s1);
        Map<String, Integer> profile2 = getProfile(s2);

        return dotProduct(profile1, profile2)
                / (norm(profile1) * norm(profile2));
    }

    /**
     * Compute the norm L2 : sqrt(Sum_i( v_i²)).
     *
     * @param profile
     * @return L2 norm
     */
    private static double norm(final Map<String, Integer> profile) {
        double agg = 0;

        for (Map.Entry<String, Integer> entry : profile.entrySet()) {
            agg += 1.0 * entry.getValue() * entry.getValue();
        }

        return Math.sqrt(agg);
    }

    private static double dotProduct(
            final Map<String, Integer> profile1,
            final Map<String, Integer> profile2) {

        // Loop over the smallest map
        Map<String, Integer> small_profile = profile2;
        Map<String, Integer> large_profile = profile1;
        if (profile1.size() < profile2.size()) {
            small_profile = profile1;
            large_profile = profile2;
        }

        double agg = 0;
        for (Map.Entry<String, Integer> entry : small_profile.entrySet()) {
            Integer i = large_profile.get(entry.getKey());
            if (i == null) {
                continue;
            }
            agg += 1.0 * entry.getValue() * i;
        }

        return agg;
    }

    /**
     * Return 1.0 - similarity.
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return 1.0 - the cosine similarity in the range [0, 1]
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        return 1.0 - similarity(s1, s2);
    }

    /**
     * {@inheritDoc}
     * @param profile1
     * @param profile2
     * @return
     */
    public final double similarity(
            final Map<String, Integer> profile1,
            final Map<String, Integer> profile2) {

        return dotProduct(profile1, profile2)
                / (norm(profile1) * norm(profile2));
    }

}
/*
 * The MIT License
 *
 * Copyright 2015 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */




/**
 * Implementation of Damerau-Levenshtein distance with transposition (also
 * sometimes calls unrestricted Damerau-Levenshtein distance).
 * It is the minimum number of operations needed to transform one string into
 * the other, where an operation is defined as an insertion, deletion, or
 * substitution of a single character, or a transposition of two adjacent
 * characters.
 * It does respect triangle inequality, and is thus a metric distance.
 *
 * This is not to be confused with the optimal string alignment distance, which
 * is an extension where no substring can be edited more than once.
 *
 * @author Thibault Debatty
 */
class Damerau implements MetricStringDistance {

    /**
     * Compute the distance between strings: the minimum number of operations
     * needed to transform one string into the other (insertion, deletion,
     * substitution of a single character, or a transposition of two adjacent
     * characters).
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed distance.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {

        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        // INFinite distance is the max possible distance
        int inf = s1.length() + s2.length();

        // Create and initialize the character array indices
        HashMap<Character, Integer> da = new HashMap<Character, Integer>();

        for (int d = 0; d < s1.length(); d++) {
            da.put(s1.charAt(d), 0);
        }

        for (int d = 0; d < s2.length(); d++) {
            da.put(s2.charAt(d), 0);
        }

        // Create the distance matrix H[0 .. s1.length+1][0 .. s2.length+1]
        int[][] h = new int[s1.length() + 2][s2.length() + 2];

        // initialize the left and top edges of H
        for (int i = 0; i <= s1.length(); i++) {
            h[i + 1][0] = inf;
            h[i + 1][1] = i;
        }

        for (int j = 0; j <= s2.length(); j++) {
            h[0][j + 1] = inf;
            h[1][j + 1] = j;

        }

        // fill in the distance matrix H
        // look at each character in s1
        for (int i = 1; i <= s1.length(); i++) {
            int db = 0;

            // look at each character in b
            for (int j = 1; j <= s2.length(); j++) {
                int i1 = da.get(s2.charAt(j - 1));
                int j1 = db;

                int cost = 1;
                if (s1.charAt(i - 1) == s2.charAt(j - 1)) {
                    cost = 0;
                    db = j;
                }

                h[i + 1][j + 1] = min(
                        h[i][j] + cost, // substitution
                        h[i + 1][j] + 1, // insertion
                        h[i][j + 1] + 1, // deletion
                        h[i1][j1] + (i - i1 - 1) + 1 + (j - j1 - 1));
            }

            da.put(s1.charAt(i - 1), i);
        }

        return h[s1.length() + 1][s2.length() + 1];
    }

    private static int min(
            final int a, final int b, final int c, final int d) {
        return Math.min(a, Math.min(b, Math.min(c, d)));
    }

}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */





/**
 * Each input string is converted into a set of n-grams, the Jaccard index is
 * then computed as |V1 inter V2| / |V1 union V2|.
 * Like Q-Gram distance, the input strings are first converted into sets of
 * n-grams (sequences of n characters, also called k-shingles), but this time
 * the cardinality of each n-gram is not taken into account.
 * Distance is computed as 1 - cosine similarity.
 * Jaccard index is a metric distance.
 * @author Thibault Debatty
 */
class Jaccard extends ShingleBased implements
        MetricStringDistance, NormalizedStringDistance,
        NormalizedStringSimilarity {

    /**
     * The strings are first transformed into sets of k-shingles (sequences of k
     * characters), then Jaccard index is computed as |A inter B| / |A union B|.
     * The default value of k is 3.
     *
     * @param k
     */
    public Jaccard(final int k) {
        super(k);
    }

    /**
     * The strings are first transformed into sets of k-shingles (sequences of k
     * characters), then Jaccard index is computed as |A inter B| / |A union B|.
     * The default value of k is 3.
     */
    public Jaccard() {
        super();
    }

    /**
     * Compute Jaccard index: |A inter B| / |A union B|.
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The Jaccard index in the range [0, 1]
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double similarity(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 1;
        }

        Map<String, Integer> profile1 = getProfile(s1);
        Map<String, Integer> profile2 = getProfile(s2);


        Set<String> union = new HashSet<String>();
        union.addAll(profile1.keySet());
        union.addAll(profile2.keySet());

        int inter = profile1.keySet().size() + profile2.keySet().size()
                - union.size();

        return 1.0 * inter / union.size();
    }


    /**
     * Distance is computed as 1 - similarity.
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return 1 - the Jaccard similarity.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        return 1.0 - similarity(s1, s2);
    }
}




/**
 * The Jaro–Winkler distance metric is designed and best suited for short
 * strings such as person names, and to detect typos; it is (roughly) a
 * variation of Damerau-Levenshtein, where the substitution of 2 close
 * characters is considered less important then the substitution of 2 characters
 * that a far from each other.
 * Jaro-Winkler was developed in the area of record linkage (duplicate
 * detection) (Winkler, 1990). It returns a value in the interval [0.0, 1.0].
 * The distance is computed as 1 - Jaro-Winkler similarity.
 * @author Thibault Debatty
 */
class JaroWinkler
        implements NormalizedStringSimilarity, NormalizedStringDistance {

    private static final double DEFAULT_THRESHOLD = 0.7;
    private static final int THREE = 3;
    private static final double JW_COEF = 0.1;
    private final double threshold;

    /**
     * Instantiate with default threshold (0.7).
     *
     */
    public JaroWinkler() {
        this.threshold = DEFAULT_THRESHOLD;
    }

    /**
     * Instantiate with given threshold to determine when Winkler bonus should
     * be used.
     * Set threshold to a negative value to get the Jaro distance.
     * @param threshold
     */
    public JaroWinkler(final double threshold) {
        this.threshold = threshold;
    }

    /**
     * Returns the current value of the threshold used for adding the Winkler
     * bonus. The default value is 0.7.
     *
     * @return the current value of the threshold
     */
    public final double getThreshold() {
        return threshold;
    }

    /**
     * Compute Jaro-Winkler similarity.
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The Jaro-Winkler similarity in the range [0, 1]
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double similarity(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 1;
        }

        int[] mtp = matches(s1, s2);
        float m = mtp[0];
        if (m == 0) {
            return 0f;
        }
        double j = ((m / s1.length() + m / s2.length() + (m - mtp[1]) / m))
                / THREE;
        double jw = j;

        if (j > getThreshold()) {
            jw = j + Math.min(JW_COEF, 1.0 / mtp[THREE]) * mtp[2] * (1 - j);
        }
        return jw;
    }


    /**
     * Return 1 - similarity.
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return 1 - similarity.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        return 1.0 - similarity(s1, s2);
    }

    private int[] matches(final String s1, final String s2) {
        String max, min;
        if (s1.length() > s2.length()) {
            max = s1;
            min = s2;
        } else {
            max = s2;
            min = s1;
        }
        int range = Math.max(max.length() / 2 - 1, 0);
        int[] match_indexes = new int[min.length()];
        Arrays.fill(match_indexes, -1);
        boolean[] match_flags = new boolean[max.length()];
        int matches = 0;
        for (int mi = 0; mi < min.length(); mi++) {
            char c1 = min.charAt(mi);
            for (int xi = Math.max(mi - range, 0),
                    xn = Math.min(mi + range + 1, max.length());
                    xi < xn;
                    xi++) {
                if (!match_flags[xi] && c1 == max.charAt(xi)) {
                    match_indexes[mi] = xi;
                    match_flags[xi] = true;
                    matches++;
                    break;
                }
            }
        }
        char[] ms1 = new char[matches];
        char[] ms2 = new char[matches];
        for (int i = 0, si = 0; i < min.length(); i++) {
            if (match_indexes[i] != -1) {
                ms1[si] = min.charAt(i);
                si++;
            }
        }
        for (int i = 0, si = 0; i < max.length(); i++) {
            if (match_flags[i]) {
                ms2[si] = max.charAt(i);
                si++;
            }
        }
        int transpositions = 0;
        for (int mi = 0; mi < ms1.length; mi++) {
            if (ms1[mi] != ms2[mi]) {
                transpositions++;
            }
        }
        int prefix = 0;
        for (int mi = 0; mi < min.length(); mi++) {
            if (s1.charAt(mi) == s2.charAt(mi)) {
                prefix++;
            } else {
                break;
            }
        }
        return new int[]{matches, transpositions / 2, prefix, max.length()};
    }
}



/**
 * The Levenshtein distance between two words is the minimum number of
 * single-character edits (insertions, deletions or substitutions) required to
 * change one string into the other.
 *
 * @author Thibault Debatty
 */
class Levenshtein implements MetricStringDistance {

    /**
     * The Levenshtein distance, or edit distance, between two words is the
     * minimum number of single-character edits (insertions, deletions or
     * substitutions) required to change one word into the other.
     *
     * http://en.wikipedia.org/wiki/Levenshtein_distance
     *
     * It is always at least the difference of the sizes of the two strings.
     * It is at most the length of the longer string.
     * It is zero if and only if the strings are equal.
     * If the strings are the same size, the Hamming distance is an upper bound
     * on the Levenshtein distance.
     * The Levenshtein distance verifies the triangle inequality (the distance
     * between two strings is no greater than the sum Levenshtein distances from
     * a third string).
     *
     * Implementation uses dynamic programming (Wagner–Fischer algorithm), with
     * only 2 rows of data. The space requirement is thus O(m) and the algorithm
     * runs in O(mn).
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed Levenshtein distance.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        if (s1.length() == 0) {
            return s2.length();
        }

        if (s2.length() == 0) {
            return s1.length();
        }

        // create two work vectors of integer distances
        int[] v0 = new int[s2.length() + 1];
        int[] v1 = new int[s2.length() + 1];
        int[] vtemp;

        // initialize v0 (the previous row of distances)
        // this row is A[0][i]: edit distance for an empty s
        // the distance is just the number of characters to delete from t
        for (int i = 0; i < v0.length; i++) {
            v0[i] = i;
        }

        for (int i = 0; i < s1.length(); i++) {
            // calculate v1 (current row distances) from the previous row v0
            // first element of v1 is A[i+1][0]
            //   edit distance is delete (i+1) chars from s to match empty t
            v1[0] = i + 1;

            // use formula to fill in the rest of the row
            for (int j = 0; j < s2.length(); j++) {
                int cost = 1;
                if (s1.charAt(i) == s2.charAt(j)) {
                    cost = 0;
                }
                v1[j + 1] = Math.min(
                        v1[j] + 1,              // Cost of insertion
                        Math.min(
                                v0[j + 1] + 1,  // Cost of remove
                                v0[j] + cost)); // Cost of substitution
            }

            // copy v1 (current row) to v0 (previous row) for next iteration
            //System.arraycopy(v1, 0, v0, 0, v0.length);

            // Flip references to current and previous row
            vtemp = v0;
            v0 = v1;
            v1 = vtemp;

        }

        return v0[s2.length()];
    }
}



/**
 * The longest common subsequence (LCS) problem consists in finding the longest
 * subsequence common to two (or more) sequences. It differs from problems of
 * finding common substrings: unlike substrings, subsequences are not required
 * to occupy consecutive positions within the original sequences.
 *
 * It is used by the diff utility, by Git for reconciling multiple changes, etc.
 *
 * The LCS distance between Strings X (length n) and Y (length m) is n + m - 2
 * |LCS(X, Y)| min = 0 max = n + m
 *
 * LCS distance is equivalent to Levenshtein distance, when only insertion and
 * deletion is allowed (no substitution), or when the cost of the substitution
 * is the double of the cost of an insertion or deletion.
 *
 * ! This class currently implements the dynamic programming approach, which has
 * a space requirement O(m * n)!
 *
 * @author Thibault Debatty
 */
class LongestCommonSubsequence implements StringDistance {

    /**
     * Return the LCS distance between strings s1 and s2, computed as |s1| +
     * |s2| - 2 * |LCS(s1, s2)|.
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return the LCS distance between strings s1 and s2, computed as |s1| +
     * |s2| - 2 * |LCS(s1, s2)|
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        return s1.length() + s2.length() - 2 * length(s1, s2);
    }

    /**
     * Return the length of Longest Common Subsequence (LCS) between strings s1
     * and s2.
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return the length of LCS(s1, s2)
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final int length(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        /* function LCSLength(X[1..m], Y[1..n])
         C = array(0..m, 0..n)

         for i := 0..m
         C[i,0] = 0

         for j := 0..n
         C[0,j] = 0

         for i := 1..m
         for j := 1..n
         if X[i] = Y[j]
         C[i,j] := C[i-1,j-1] + 1
         else
         C[i,j] := max(C[i,j-1], C[i-1,j])
         return C[m,n]
         */
        int s1_length = s1.length();
        int s2_length = s2.length();
        char[] x = s1.toCharArray();
        char[] y = s2.toCharArray();

        int[][] c = new int[s1_length + 1][s2_length + 1];

        for (int i = 1; i <= s1_length; i++) {
            for (int j = 1; j <= s2_length; j++) {
                if (x[i - 1] == y[j - 1]) {
                    c[i][j] = c[i - 1][j - 1] + 1;

                } else {
                    c[i][j] = Math.max(c[i][j - 1], c[i - 1][j]);
                }
            }
        }

        return c[s1_length][s2_length];
    }
}
/*
 * The MIT License
 *
 * Copyright 2015 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */




/**
 * Distance metric based on Longest Common Subsequence, from the notes "An
 * LCS-based string metric" by Daniel Bakkelund.
 *
 * @author Thibault Debatty
 */
class MetricLCS
        implements MetricStringDistance, NormalizedStringDistance {

    private final LongestCommonSubsequence lcs = new LongestCommonSubsequence();

    /**
     * Distance metric based on Longest Common Subsequence, computed as
     * 1 - |LCS(s1, s2)| / max(|s1|, |s2|).
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed distance metric value.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        int m_len = Math.max(s1.length(), s2.length());
        if (m_len == 0) {
            return 0;
        }
        return 1.0
            - (1.0 * lcs.length(s1, s2))
            / m_len;
    }
}



/**
 * N-Gram Similarity as defined by Kondrak, "N-Gram Similarity and Distance",
 * String Processing and Information Retrieval, Lecture Notes in Computer
 * Science Volume 3772, 2005, pp 115-126.
 *
 * The algorithm uses affixing with special character '\n' to increase the
 * weight of first characters. The normalization is achieved by dividing the
 * total similarity score the original length of the longest word.
 *
 * http://webdocs.cs.ualberta.ca/~kondrak/papers/spire05.pdf
 */
class NGram implements NormalizedStringDistance {

    private static final int DEFAULT_N = 2;
    private final int n;

    /**
     * Instantiate with given value for n-gram length.
     * @param n
     */
    public NGram(final int n) {
        this.n = n;
    }

    /**
     * Instantiate with default value for n-gram length (2).
     */
    public NGram() {
        this.n = DEFAULT_N;
    }

    /**
     * Compute n-gram distance.
     * @param s0 The first string to compare.
     * @param s1 The second string to compare.
     * @return The computed n-gram distance in the range [0, 1]
     * @throws NullPointerException if s0 or s1 is null.
     */
    public final double distance(final String s0, final String s1) {
        if (s0 == null) {
            throw new NullPointerException("s0 must not be null");
        }

        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s0.equals(s1)) {
            return 0;
        }

        final char special = '\n';
        final int sl = s0.length();
        final int tl = s1.length();

        if (sl == 0 || tl == 0) {
            return 1;
        }

        int cost = 0;
        if (sl < n || tl < n) {
            for (int i = 0, ni = Math.min(sl, tl); i < ni; i++) {
                if (s0.charAt(i) == s1.charAt(i)) {
                    cost++;
                }
            }
            return (float) cost / Math.max(sl, tl);
        }

        char[] sa = new char[sl + n - 1];
        float[] p; //'previous' cost array, horizontally
        float[] d; // cost array, horizontally
        float[] d2; //placeholder to assist in swapping p and d

        //construct sa with prefix
        for (int i = 0; i < sa.length; i++) {
            if (i < n - 1) {
                sa[i] = special; //add prefix
            } else {
                sa[i] = s0.charAt(i - n + 1);
            }
        }
        p = new float[sl + 1];
        d = new float[sl + 1];

        // indexes into strings s and t
        int i; // iterates through source
        int j; // iterates through target

        char[] t_j = new char[n]; // jth n-gram of t

        for (i = 0; i <= sl; i++) {
            p[i] = i;
        }

        for (j = 1; j <= tl; j++) {
            //construct t_j n-gram
            if (j < n) {
                for (int ti = 0; ti < n - j; ti++) {
                    t_j[ti] = special; //add prefix
                }
                for (int ti = n - j; ti < n; ti++) {
                    t_j[ti] = s1.charAt(ti - (n - j));
                }
            } else {
                t_j = s1.substring(j - n, j).toCharArray();
            }
            d[0] = j;
            for (i = 1; i <= sl; i++) {
                cost = 0;
                int tn = n;
                //compare sa to t_j
                for (int ni = 0; ni < n; ni++) {
                    if (sa[i - 1 + ni] != t_j[ni]) {
                        cost++;
                    } else if (sa[i - 1 + ni] == special) {
                        //discount matches on prefix
                        tn--;
                    }
                }
                float ec = (float) cost / tn;
                // minimum of cell to the left+1, to the top+1,
                // diagonally left and up +cost
                d[i] = Math.min(
                        Math.min(d[i - 1] + 1, p[i] + 1), p[i - 1] + ec);
            }
            // copy current distance counts to 'previous row' distance counts
            d2 = p;
            p = d;
            d = d2;
        }

        // our last action in the above loop was to switch d and p, so p now
        // actually has the most recent cost counts
        return p[sl] / Math.max(tl, sl);
    }
}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */



/**
 * This distance is computed as levenshtein distance divided by the length of
 * the longest string. The resulting value is always in the interval [0.0 1.0]
 * but it is not a metric anymore! The similarity is computed as 1 - normalized
 * distance.
 *
 * @author Thibault Debatty
 */
class NormalizedLevenshtein implements
        NormalizedStringDistance, NormalizedStringSimilarity {

    private final Levenshtein l = new Levenshtein();

    /**
     * Compute distance as Levenshtein(s1, s2) / max(|s1|, |s2|).
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed distance in the range [0, 1]
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {

        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        int m_len = Math.max(s1.length(), s2.length());

        if (m_len == 0) {
            return 0;
        }

        return l.distance(s1, s2) / m_len;
    }

    /**
     * Return 1 - distance.
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return 1.0 - the computed distance
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double similarity(final String s1, final String s2) {
        return 1.0 - distance(s1, s2);
    }

}
/*
 * The MIT License
 *
 * Copyright 2016 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */



/**
 * Implementation of the the Optimal String Alignment (sometimes called the
 * restricted edit distance) variant of the Damerau-Levenshtein distance.
 *
 * The difference between the two algorithms consists in that the Optimal String
 * Alignment algorithm computes the number of edit operations needed to make the
 * strings equal under the condition that no substring is edited more than once,
 * whereas Damerau-Levenshtein presents no such restriction.
 *
 * @author Michail Bogdanos
 */
final class OptimalStringAlignment implements StringDistance {

    /**
     * Compute the distance between strings: the minimum number of operations
     * needed to transform one string into the other (insertion, deletion,
     * substitution of a single character, or a transposition of two adjacent
     * characters) while no substring is edited more than once.
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return the OSA distance
     * @throws NullPointerException if s1 or s2 is null.
     */
    public double distance(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        int n = s1.length(), m = s2.length();

        if (n == 0) {
            return m;
        }

        if (m == 0) {
            return n;
        }

        // Create the distance matrix H[0 .. s1.length+1][0 .. s2.length+1]
        int[][] d = new int[n + 2][m + 2];

        //initialize top row and leftmost column
        for (int i = 0; i <= n; i++) {
            d[i][0] = i;
        }
        for (int j = 0; j <= m; j++) {
            d[0][j] = j;
        }

        //fill the distance matrix
        int cost;

        for (int i = 1; i <= n; i++) {
            for (int j = 1; j <= m; j++) {

                //if s1[i - 1] = s2[j - 1] then cost = 0, else cost = 1
                cost = 1;
                if (s1.charAt(i - 1) == s2.charAt(j - 1)) {
                    cost = 0;
                }

                d[i][j] = min(
                        d[i - 1][j - 1] + cost, // substitution
                        d[i][j - 1] + 1, // insertion
                        d[i - 1][j] + 1 // deletion
                );

                //transposition check
                if (i > 1 && j > 1
                        && s1.charAt(i - 1) == s2.charAt(j - 2)
                        && s1.charAt(i - 2) == s2.charAt(j - 1)) {
                    d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + cost);
                }
            }
        }

        return d[n][m];
    }

    private static int min(
            final int a, final int b, final int c) {
        return Math.min(a, Math.min(b, c));
    }
}





/**
 * Q-gram distance, as defined by Ukkonen in "Approximate string-matching with
 * q-grams and maximal matches". The distance between two strings is defined as
 * the L1 norm of the difference of their profiles (the number of occurences of
 * each n-gram): SUM( |V1_i - V2_i| ). Q-gram distance is a lower bound on
 * Levenshtein distance, but can be computed in O(m + n), where Levenshtein
 * requires O(m.n).
 *
 * @author Thibault Debatty
 */
class QGram extends ShingleBased implements StringDistance {

    /**
     * Q-gram similarity and distance. Defined by Ukkonen in "Approximate
     * string-matching with q-grams and maximal matches",
     * http://www.sciencedirect.com/science/article/pii/0304397592901434 The
     * distance between two strings is defined as the L1 norm of the difference
     * of their profiles (the number of occurences of each k-shingle). Q-gram
     * distance is a lower bound on Levenshtein distance, but can be computed in
     * O(|A| + |B|), where Levenshtein requires O(|A|.|B|)
     *
     * @param k
     */
    public QGram(final int k) {
        super(k);
    }

    /**
     * Q-gram similarity and distance. Defined by Ukkonen in "Approximate
     * string-matching with q-grams and maximal matches",
     * http://www.sciencedirect.com/science/article/pii/0304397592901434 The
     * distance between two strings is defined as the L1 norm of the difference
     * of their profiles (the number of occurence of each k-shingle). Q-gram
     * distance is a lower bound on Levenshtein distance, but can be computed in
     * O(|A| + |B|), where Levenshtein requires O(|A|.|B|)
     * Default k is 3.
     */
    public QGram() {
        super();
    }

    /**
     * The distance between two strings is defined as the L1 norm of the
     * difference of their profiles (the number of occurence of each k-shingle).
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed Q-gram distance.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        Map<String, Integer> profile1 = getProfile(s1);
        Map<String, Integer> profile2 = getProfile(s2);

        return distance(profile1, profile2);
    }

    /**
     * Compute QGram distance using precomputed profiles.
     *
     * @param profile1
     * @param profile2
     * @return
     */
    public final double distance(
            final Map<String, Integer> profile1,
            final Map<String, Integer> profile2) {

        Set<String> union = new HashSet<String>();
        union.addAll(profile1.keySet());
        union.addAll(profile2.keySet());

        int agg = 0;
        for (String key : union) {
            int v1 = 0;
            int v2 = 0;
            Integer iv1 = profile1.get(key);
            if (iv1 != null) {
                v1 = iv1;
            }

            Integer iv2 = profile2.get(key);
            if (iv2 != null) {
                v2 = iv2;
            }
            agg += Math.abs(v1 - v2);
        }
        return agg;
    }
}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */




/**
 * Abstract class for string similarities that rely on set operations (like
 * cosine similarity or jaccard index).
 *
 * k-shingling is the operation of transforming a string (or text document) into
 * a set of n-grams, which can be used to measure the similarity between two
 * strings or documents.
 *
 * Generally speaking, a k-gram is any sequence of k tokens. We use here the
 * definition from Leskovec, Rajaraman & Ullman (2014), "Mining of Massive
 * Datasets", Cambridge University Press: Multiple subsequent spaces are
 * replaced by a single space, and a k-gram is a sequence of k characters.
 *
 * Default value of k is 3. A good rule of thumb is to imagine that there are
 * only 20 characters and estimate the number of k-shingles as 20^k. For small
 * documents like e-mails, k = 5 is a recommended value. For large documents,
 * such as research articles, k = 9 is considered a safe choice.
 *
 * @author Thibault Debatty
 */
abstract class ShingleBased {

    private static final int DEFAULT_K = 3;

    private final int k;

    /**
     * Pattern for finding multiple following spaces.
     */
    private static final Pattern SPACE_REG = Pattern.compile("\\s+");

    /**
     *
     * @param k
     * @throws IllegalArgumentException if k is <= 0
     */
    ShingleBased(final int k) {
        if (k <= 0) {
            throw new IllegalArgumentException("k should be positive!");
        }
        this.k = k;
    }

    /**
     *
     */
    ShingleBased() {
        this(DEFAULT_K);
    }

    /**
     * Return k, the length of k-shingles (aka n-grams).
     *
     * @return The length of k-shingles.
     */
    public int getK() {
        return k;
    }

    /**
     * Compute and return the profile of s, as defined by Ukkonen "Approximate
     * string-matching with q-grams and maximal matches".
     * https://www.cs.helsinki.fi/u/ukkonen/TCS92.pdf The profile is the number
     * of occurrences of k-shingles, and is used to compute q-gram similarity,
     * Jaccard index, etc. Pay attention: the memory requirement of the profile
     * can be up to k * size of the string
     *
     * @param string
     * @return the profile of this string, as an unmodifiable Map
     */
    public final Map<String, Integer> getProfile(final String string) {
        HashMap<String, Integer> shingles = new HashMap<String, Integer>();

        String string_no_space = SPACE_REG.matcher(string).replaceAll(" ");
        for (int i = 0; i < (string_no_space.length() - k + 1); i++) {
            String shingle = string_no_space.substring(i, i + k);
            Integer old = shingles.get(shingle);
            if (old != null) {
                shingles.put(shingle, old + 1);
            } else {
                shingles.put(shingle, 1);
            }
        }

        return Collections.unmodifiableMap(shingles);
    }

}
/*
 * The MIT License
 *
 * Copyright 2015 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */




/**
 * Similar to Jaccard index, but this time the similarity is computed as 2 * |V1
 * inter V2| / (|V1| + |V2|). Distance is computed as 1 - cosine similarity.
 *
 * @author Thibault Debatty
 */
class SorensenDice extends ShingleBased implements
        NormalizedStringDistance, NormalizedStringSimilarity {

    /**
     * Sorensen-Dice coefficient, aka Sørensen index, Dice's coefficient or
     * Czekanowski's binary (non-quantitative) index.
     *
     * The strings are first converted to boolean sets of k-shingles (sequences
     * of k characters), then the similarity is computed as 2 * |A inter B| /
     * (|A| + |B|). Attention: Sorensen-Dice distance (and similarity) does not
     * satisfy triangle inequality.
     *
     * @param k
     */
    public SorensenDice(final int k) {
        super(k);
    }

    /**
     * Sorensen-Dice coefficient, aka Sørensen index, Dice's coefficient or
     * Czekanowski's binary (non-quantitative) index.
     *
     * The strings are first converted to boolean sets of k-shingles (sequences
     * of k characters), then the similarity is computed as 2 * |A inter B| /
     * (|A| + |B|). Attention: Sorensen-Dice distance (and similarity) does not
     * satisfy triangle inequality. Default k is 3.
     */
    public SorensenDice() {
        super();
    }

    /**
     * Similarity is computed as 2 * |A inter B| / (|A| + |B|).
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed Sorensen-Dice similarity.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double similarity(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 1;
        }

        Map<String, Integer> profile1 = getProfile(s1);
        Map<String, Integer> profile2 = getProfile(s2);

        Set<String> union = new HashSet<String>();
        union.addAll(profile1.keySet());
        union.addAll(profile2.keySet());

        int inter = 0;

        for (String key : union) {
            if (profile1.containsKey(key) && profile2.containsKey(key)) {
                inter++;
            }
        }

        return 2.0 * inter / (profile1.size() + profile2.size());
    }

    /**
     * Returns 1 - similarity.
     *
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return 1.0 - the computed similarity
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        return 1 - similarity(s1, s2);
    }
}
/*
 * The MIT License
 *
 * Copyright 2015 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */



/**
 * Implementation of Levenshtein that allows to define different weights for
 * different character substitutions.
 *
 * @author Thibault Debatty
 */
class WeightedLevenshtein implements StringDistance {

    private final CharacterSubstitutionInterface charsub;

    /**
     * Instatiate with provided character substitution.
     * @param charsub The strategy to determine character substitution weights.
     */
    public WeightedLevenshtein(final CharacterSubstitutionInterface charsub) {
        this.charsub = charsub;
    }

    /**
     * Compute Levenshtein distance using provided weights for substitution.
     * @param s1 The first string to compare.
     * @param s2 The second string to compare.
     * @return The computed weighted Levenshtein distance.
     * @throws NullPointerException if s1 or s2 is null.
     */
    public final double distance(final String s1, final String s2) {
        if (s1 == null) {
            throw new NullPointerException("s1 must not be null");
        }

        if (s2 == null) {
            throw new NullPointerException("s2 must not be null");
        }

        if (s1.equals(s2)) {
            return 0;
        }

        if (s1.length() == 0) {
            return s2.length();
        }

        if (s2.length() == 0) {
            return s1.length();
        }

        // create two work vectors of integer distances
        double[] v0 = new double[s2.length() + 1];
        double[] v1 = new double[s2.length() + 1];
        double[] vtemp;

        // initialize v0 (the previous row of distances)
        // this row is A[0][i]: edit distance for an empty s
        // the distance is just the number of characters to delete from t
        for (int i = 0; i < v0.length; i++) {
            v0[i] = i;
        }

        for (int i = 0; i < s1.length(); i++) {
            // calculate v1 (current row distances) from the previous row v0
            // first element of v1 is A[i+1][0]
            //   edit distance is delete (i+1) chars from s to match empty t
            v1[0] = i + 1;

            // use formula to fill in the rest of the row
            for (int j = 0; j < s2.length(); j++) {
                double cost = 0;
                if (s1.charAt(i) != s2.charAt(j)) {
                    cost = charsub.cost(s1.charAt(i), s2.charAt(j));
                }
                v1[j + 1] = Math.min(
                        v1[j] + 1, // Cost of insertion
                        Math.min(
                                v0[j + 1] + 1, // Cost of remove
                                v0[j] + cost)); // Cost of substitution
            }

            // copy v1 (current row) to v0 (previous row) for next iteration
            //System.arraycopy(v1, 0, v0, 0, v0.length);
            // Flip references to current and previous row
            vtemp = v0;
            v0 = v1;
            v1 = vtemp;

        }

        return v0[s2.length()];
    }
}
/*
 * The MIT License
 *
 * Copyright 2016 Thibault Debatty.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */



/**
 * Sift4 - a general purpose string distance algorithm inspired by JaroWinkler
 * and Longest Common Subsequence.
 * Original JavaScript algorithm by siderite, java port by Nathan Fischer 2016.
 * https://siderite.blogspot.com/2014/11/super-fast-and-accurate-string-
 * distance.html
 *
 * @author Thibault Debatty
 */
class Sift4 implements StringDistance {

    private static final int DEFAULT_MAX_OFFSET = 10;

    private int max_offset = DEFAULT_MAX_OFFSET;

    /**
     * Set the maximum distance to search for character transposition.
     * Compute cost of algorithm is O(n . max_offset)
     * @param max_offset
     */
    public final void setMaxOffset(final int max_offset) {
        this.max_offset = max_offset;
    }

    /**
     * Sift4 - a general purpose string distance algorithm inspired by
     * JaroWinkler and Longest Common Subsequence.
     * Original JavaScript algorithm by siderite, java port by Nathan Fischer
     * 2016.
     * https://siderite.blogspot.com/2014/11/super-fast-and-accurate-string-
     * distance.html
     *
     * @param s1
     * @param s2
     * @return
     */
    public final double distance(final String s1, final String s2) {

        /**
         * Used to store relation between same character in different positions
         * c1 and c2 in the input strings.
         */
        class Offset {

            private final int c1;
            private final int c2;
            private boolean trans;

            Offset(final int c1, final int c2, final boolean trans) {
                this.c1 = c1;
                this.c2 = c2;
                this.trans = trans;
            }
        }

        if (s1 == null || s1.isEmpty()) {
            if (s2 == null) {
                return 0;
            }

            return s2.length();
        }

        if (s2 == null || s2.isEmpty()) {
            return s1.length();
        }

        int l1 = s1.length();
        int l2 = s2.length();

        int c1 = 0;  //cursor for string 1
        int c2 = 0;  //cursor for string 2
        int lcss = 0;  //largest common subsequence
        int local_cs = 0; //local common substring
        int trans = 0;  //number of transpositions ('ab' vs 'ba')

        // offset pair array, for computing the transpositions
        LinkedList<Offset> offset_arr = new LinkedList<Offset>();

        while ((c1 < l1) && (c2 < l2)) {
            if (s1.charAt(c1) == s2.charAt(c2)) {
                local_cs++;
                boolean is_trans = false;
                // see if current match is a transposition
                int i = 0;
                while (i < offset_arr.size()) {
                    Offset ofs = offset_arr.get(i);
                    if (c1 <= ofs.c1 || c2 <= ofs.c2) {
                        // when two matches cross, the one considered a
                        // transposition is the one with the largest difference
                        // in offsets
                        is_trans =
                                Math.abs(c2 - c1) >= Math.abs(ofs.c2 - ofs.c1);
                        if (is_trans) {

                            trans++;
                        } else {
                            if (!ofs.trans) {
                                ofs.trans = true;
                                trans++;
                            }
                        }

                        break;
                    } else {
                        if (c1 > ofs.c2 && c2 > ofs.c1) {
                            offset_arr.remove(i);
                        } else {
                            i++;
                        }
                    }
                }
                offset_arr.add(new Offset(c1, c2, is_trans));

            } else {

                // s1.charAt(c1) != s2.charAt(c2)
                lcss += local_cs;
                local_cs = 0;
                if (c1 != c2) {
                    //using min allows the computation of transpositions
                    c1 = Math.min(c1, c2);
                    c2 = c1;
                }

                // if matching characters are found, remove 1 from both cursors
                // (they get incremented at the end of the loop)
                // so that we can have only one code block handling matches
                for (
                        int i = 0;
                        i < max_offset && (c1 + i < l1 || c2 + i < l2);
                        i++) {

                    if ((c1 + i < l1) && (s1.charAt(c1 + i) == s2.charAt(c2))) {
                        c1 += i - 1;
                        c2--;
                        break;
                    }

                    if ((c2 + i < l2) && (s1.charAt(c1) == s2.charAt(c2 + i))) {
                        c1--;
                        c2 += i - 1;
                        break;
                    }
                }
            }
            c1++;
            c2++;
            // this covers the case where the last match is on the last token
            // in list, so that it can compute transpositions correctly
            if ((c1 >= l1) || (c2 >= l2)) {
                lcss += local_cs;
                local_cs = 0;
                c1 = Math.min(c1, c2);
                c2 = c1;
            }
        }
        lcss += local_cs;
        // add the cost of transpositions to the final result
        return Math.round(Math.max(l1, l2) - lcss + trans);
    }
}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */


/**
 * String distances that implement this interface are metrics.
 * This means:
 * - d(x, y) ≥ 0 (non-negativity, or separation axiom)
 * - d(x, y) = 0 if and only if x = y (identity, or coincidence axiom)
 * - d(x, y) = d(y, x) (symmetry)
 * - d(x, z) ≤ d(x, y) + d(y, z) (triangle inequality).
 *
 * @author Thibault Debatty
 */
interface MetricStringDistance extends StringDistance {

    /**
     * Compute and return the metric distance.
     * @param s1
     * @param s2
     * @return
     */
    double distance(String s1, String s2);
}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */


/**
 * Normalized string similarities return a similarity between 0.0 and 1.0.
 *
 * @author Thibault Debatty
 */
interface NormalizedStringDistance extends StringDistance {

}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */


/**
 *
 * @author tibo
 */
interface NormalizedStringSimilarity extends StringSimilarity {

}
/*
 * The MIT License
 *
 * Copyright 2015 tibo.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */



/**
 *
 * @author Thibault Debatty
 */
interface StringDistance extends Serializable {

    /**
     * Compute and return a measure of distance.
     * Must be >= 0.
     * @param s1
     * @param s2
     * @return
     */
    double distance(String s1, String s2);
}



/**
 *
 * @author Thibault Debatty
 */
interface StringSimilarity extends Serializable {
    /**
     * Compute and return a measure of similarity between 2 strings.
     * @param s1
     * @param s2
     * @return similarity (0 means both strings are completely different)
     */
    double similarity(String s1, String s2);
}
