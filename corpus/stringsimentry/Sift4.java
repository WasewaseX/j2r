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
package stringsimentry;

import java.util.LinkedList;

/**
 * Sift4 - a general purpose string distance algorithm inspired by JaroWinkler
 * and Longest Common Subsequence.
 * Original JavaScript algorithm by siderite, java port by Nathan Fischer 2016.
 * https://siderite.blogspot.com/2014/11/super-fast-and-accurate-string-
 * distance.html
 *
 * @author Thibault Debatty
 *
 * Reshaped from the vendored tdebatty/java-string-similarity v0.24
 * experimental source (scanaislop round-4 wave): the method-local Offset
 * class became a package-private top-level carrier, and the null guards,
 * the transposition scan and the max-offset shift loop are extracted
 * verbatim so every function stays under the 80-line house limit.
 * Arithmetic, control flow and evaluation order are unchanged
 * (byte-identity gated by StringsSimCorpus rows sift_ab/sift_emoji/
 * sift_trans).
 */
public class Sift4 implements StringDistance {

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

        int early = earlyResult(s1, s2);
        if (early >= 0) {
            return early;
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
                // see if current match is a transposition: the scan helper
                // returns the updated transposition count and carries the
                // is-transposition verdict through a one-slot boolean array
                boolean[] isTransOut = new boolean[1];
                trans = scanTranspositions(offset_arr, c1, c2, trans,
                        isTransOut);
                offset_arr.add(new Offset(c1, c2, isTransOut[0]));

            } else {

                // s1.charAt(c1) != s2.charAt(c2)
                lcss += local_cs;
                local_cs = 0;
                if (c1 != c2) {
                    //using min allows the computation of transpositions
                    c1 = Math.min(c1, c2);
                    c2 = c1;
                }

                // if matching characters are found, remove 1 from both
                // cursors (they get incremented at the end of the loop) so
                // that we can have only one code block handling matches
                int[] shift = bestShift(s1, s2, c1, c2, l1, l2);
                c1 += shift[0];
                c2 += shift[1];
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

    /**
     * The upstream null/empty guards of distance(), extracted verbatim:
     * returns the early answer (0, s2.length(), or s1.length()) or -1 when
     * the main loop must run.
     */
    private static int earlyResult(final String s1, final String s2) {
        if (s1 == null || s1.isEmpty()) {
            if (s2 == null) {
                return 0;
            }

            return s2.length();
        }

        if (s2 == null || s2.isEmpty()) {
            return s1.length();
        }

        return -1;
    }

    /**
     * The upstream transposition scan of the match branch, extracted
     * verbatim: walks the offset list, resolves crossings (the
     * transposition is the match with the largest offset difference),
     * prunes stale offsets, and carries the is-transposition verdict
     * through isTransOut[0].
     */
    private static int scanTranspositions(
            final LinkedList<Offset> offset_arr, final int c1, final int c2,
            int trans, final boolean[] isTransOut) {
        boolean is_trans = false;
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
        isTransOut[0] = is_trans;
        return trans;
    }

    /**
     * The upstream max-offset shift scan of the mismatch branch, extracted
     * verbatim: searches for a matching character within max_offset and
     * returns the relative cursor shift as {dc1, dc2} ({0, 0} when no
     * shift fires).
     */
    private int[] bestShift(final String s1, final String s2,
            final int c1, final int c2, final int l1, final int l2) {
        for (
                int i = 0;
                i < max_offset && (c1 + i < l1 || c2 + i < l2);
                i++) {

            if ((c1 + i < l1) && (s1.charAt(c1 + i) == s2.charAt(c2))) {
                return new int[] {i - 1, -1};
            }

            if ((c2 + i < l2) && (s1.charAt(c1) == s2.charAt(c2 + i))) {
                return new int[] {-1, i - 1};
            }
        }
        return new int[] {0, 0};
    }
}

/**
 * Used to store relation between same character in different positions
 * c1 and c2 in the input strings. Upstream a method-local class of
 * Sift4.distance; a package-private top-level carrier since the extracted
 * scan helper needs it (scanaislop round-4 reshape — fields
 * package-visible where the family touches them).
 */
final class Offset {

    final int c1;
    final int c2;
    boolean trans;

    Offset(final int c1, final int c2, final boolean trans) {
        this.c1 = c1;
        this.c2 = c2;
        this.trans = trans;
    }
}
