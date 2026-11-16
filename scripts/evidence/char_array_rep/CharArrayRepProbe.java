/**
 * CharArrayRepProbe — the [C representation probe (the run-2 P0 regression
 * fixture). At 58a41ae the TYPED backend crashed on StringsSimBench with
 * rc=101, runtime/src/array.rs:496 "int-group accessor on a reference
 * array" (array_load_i <- NGram.distance): String.toCharArray produced a
 * prim=None, boxed-slot [C while every newarray-produced [C carries the
 * PrimData payload the raw accessors require. The probe pins the [C laws
 * over BOTH producers (new char[n] and toCharArray) and every cross-shape
 * consumer (castore, getChars, String(char[]), copyOf, arraycopy, the
 * NGram-shaped kernel), so no future representation drift can pass the
 * three-channel battery on either backend.
 */
public class CharArrayRepProbe {
    public static void main(String[] a) {
        // c01. toCharArray: length + element reads (the P0 crash shape —
        //      caload over a toCharArray-produced [C)
        char[] c = "abcX".toCharArray();
        System.out.println("c01: len=" + c.length + " e0=" + c[0] + " e3=" + c[3]);

        // c02. castore INTO a toCharArray array (the store-side shape)
        c[1] = 'B';
        System.out.println("c02: " + new String(c) + " e1=" + (int) c[1]);

        // c03. substring().toCharArray() REASSIGNMENT in a loop (the t_j
        //      law): the same local holds both producer shapes across
        //      iterations
        char[] tj = new char[2];
        String s = "wxyz";
        for (int j = 1; j <= 3; j++) {
            if (j < 2) {
                tj[0] = '\n';
                tj[1] = s.charAt(j - 1);
            } else {
                tj = s.substring(j - 2, j).toCharArray();
            }
            System.out.println("c03: j=" + j + " t=" + tj[0] + "" + tj[1]);
        }

        // c04. new char[n] control: the payload producer, read + write
        char[] nc = new char[4];
        nc[0] = 'p'; nc[1] = 'q'; nc[2] = 'r'; nc[3] = '\n';
        System.out.println("c04: len=" + nc.length + " " + new String(nc)
            + " e3=" + (int) nc[3]);

        // c05. char[] swap across loop iterations (the NGram p/d shape
        //      with char[]): reads after the swap must see the new owner
        char[] p = "first".toCharArray();
        char[] d = "second".toCharArray();
        for (int k = 0; k < 2; k++) {
            char[] t2 = p; p = d; d = t2;
            System.out.println("c05: k=" + k + " p=" + p[0] + " d=" + d[0]);
        }

        // c06. String.getChars INTO a toCharArray array (the Value-path
        //      store on a [C destination)
        char[] dst = "hello world".toCharArray();
        "WIDE".getChars(0, 4, dst, 2);
        System.out.println("c06: " + new String(dst));

        // c07. new String(char[]) roundtrip (both producer shapes)
        System.out.println("c07: s=" + new String(c) + " v=" + new String(nc));

        // c08. manual element copy: a payload-shape destination read from
        //      a toCharArray source — the cross-shape per-element read law
        char[] c8 = new char[6];
        for (int i = 0; i < c.length; i++) c8[i] = c[i];
        System.out.println("c08: len=" + c8.length + " " + new String(c8)
            + " tail=" + (int) c8[5]);

        // c09. System.arraycopy char[] -> char[] in BOTH directions across
        //      the two representations
        char[] src = "abcdef".toCharArray();
        char[] out = new char[6];
        System.arraycopy(src, 0, out, 0, 6);
        System.out.println("c09: " + new String(out));
        char[] out2 = new char[4];
        System.arraycopy(out, 2, out2, 0, 4);
        System.out.println("c09b: " + new String(out2));

        // c10. the distilled NGram kernel: the '\n'-prefixed sa build, the
        //      p/d float[] swap, the t_j reassignment, the inner compare
        //      loop over sa + t_j (the exact shape that crashed the typed
        //      backend at 58a41ae). Printed from the dispatch-body main:
        //      the kernel itself carries NO invokedynamic concat, NO
        //      switch and NO array-literal fill — the concat would refuse
        //      the typed body (HasInvokeDynamic) and hide the bug.
        System.out.println("c10: " + ngram2("galling", "gallingly"));
        System.out.println("c10b: " + ngram2("testing", "gesturing"));
        System.out.println("c10c: " + ngram2("abc", "abc"));

        // c11. empty-string laws
        char[] e = "".toCharArray();
        System.out.println("c11: len=" + e.length + " s=" + new String(e));
        System.out.println("c11b: " + ngram2("", "abc"));

        // c12. supplementary characters: one code point = two UTF-16 units
        char[] sup = "\uD83D\uDE00x".toCharArray();
        System.out.println("c12: len=" + sup.length + " u0=" + (int) sup[0]
            + " u2=" + (int) sup[2]);

        // c13. element-wise StringBuilder.append(char) over BOTH producer
        //      shapes — the per-element read law at every index
        StringBuilder sb = new StringBuilder();
        for (char ch : c) sb.append(ch);
        for (char ch : nc) sb.append(ch);
        System.out.println("c13: " + sb);

        // c14. NPE law: toCharArray on a null String — the NPE FAMILY is
        //      pinned here; the HELPFUL-NPE message law is the registered
        //      residuals-wave slice, deliberately not double-pinned
        try {
            char[] bad = null;
            bad = badNA().toCharArray();
            System.out.println("c14: unreachable " + bad.length);
        } catch (NullPointerException npe) {
            System.out.println("c14: " + npe.getClass().getName());
        }

        // c15. bounds: castore past the end on a toCharArray array — the
        //      JDK bounds message family
        try {
            char[] b = "ab".toCharArray();
            b[2] = 'z';
            System.out.println("c15: unreachable");
        } catch (ArrayIndexOutOfBoundsException obe) {
            System.out.println("c15: msg=" + obe.getMessage());
        }
    }

    static String badNA() { return null; }

    /** NGram.distance n=2 distilled: the faithful shape (returns double,
     *  no concat/switch/fill — the typed body must be attempted exactly
     *  like the real NGram.distance). */
    static double ngram2(String s0, String s1) {
        if (s0.equals(s1)) return 0;
        final char special = '\n';
        final int n = 2;
        int sl = s0.length(), tl = s1.length();
        if (sl == 0 || tl == 0) return 1;
        int cost = 0;
        if (sl < n || tl < n) {
            for (int i = 0, ni = Math.min(sl, tl); i < ni; i++) {
                if (s0.charAt(i) == s1.charAt(i)) cost++;
            }
            return (float) cost / Math.max(sl, tl);
        }
        char[] sa = new char[sl + n - 1];
        for (int i = 0; i < sa.length; i++) {
            sa[i] = (i < n - 1) ? special : s0.charAt(i - n + 1);
        }
        float[] p = new float[sl + 1];
        float[] d = new float[sl + 1];
        int i, j;
        char[] t_j = new char[n];
        for (i = 0; i <= sl; i++) p[i] = i;
        for (j = 1; j <= tl; j++) {
            if (j < n) {
                for (int ti = 0; ti < n - j; ti++) t_j[ti] = special;
                for (int ti = n - j; ti < n; ti++) t_j[ti] = s1.charAt(ti - (n - j));
            } else {
                t_j = s1.substring(j - n, j).toCharArray();
            }
            d[0] = j;
            for (i = 1; i <= sl; i++) {
                cost = 0;
                int tn = n;
                for (int ni = 0; ni < n; ni++) {
                    if (sa[i - 1 + ni] != t_j[ni]) {
                        cost++;
                    } else if (sa[i - 1 + ni] == special) {
                        tn--;
                    }
                }
                float ec = (float) cost / tn;
                d[i] = Math.min(Math.min(d[i - 1] + 1, p[i] + 1), p[i - 1] + ec);
            }
            float[] d2 = p; p = d; d = d2;
        }
        return p[sl] / Math.max(tl, sl);
    }
}
