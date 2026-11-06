/**
 * Temurin vector for deep primitive-array descriptors (`[[[I` and deeper —
 * the registered "`[[[<prim>` verifier depth" gap): allocation with partial
 * dims, element store/load across levels, length at each level, clone(),
 * System.arraycopy of 3D arrays, null-row NPEs, instanceof, and the
 * aastore component check on deep arrays.
 */
public class DeepArrays {

    public static void main(String[] args) {
        // 1. full-dim allocation + nested length
        int[][][] cube = new int[2][3][4];
        System.out.println(cube.length + " " + cube[0].length + " " + cube[0][0].length);

        // 2. nested store/load: the leaf is an int, the middle a matrix
        cube[1][2][3] = 42;
        cube[0][0][0] = cube[1][2][3] - 40;
        System.out.println(cube[1][2][3] + " " + cube[0][0][0]);

        // 3. partial allocation: leaf rows are null until assigned
        int[][][] half = new int[2][3][];
        System.out.println(half.length + " " + half[0].length + " " + (half[1][2] == null));
        half[1][2] = new int[]{7, 8, 9};
        System.out.println(half[1][2][1]);

        // 4. one-dim-only allocation: every row is null
        int[][][] tops = new int[3][][];
        System.out.println(tops.length + " " + (tops[0] == null));
        tops[0] = new int[2][2];
        tops[0][1][1] = 5;
        System.out.println(tops[0][1][1]);

        // 5. clone() at depth: the outer array clones, leaf int[]s share
        int[][][] orig = new int[1][1][1];
        orig[0][0][0] = 9;
        int[][][] cloned = orig.clone();
        cloned[0][0][0] = 11;
        System.out.println(orig[0][0][0] + " " + (cloned != orig) + " " + (cloned[0] == orig[0]));

        // 6. arraycopy of a 3D array (reference semantics per element)
        int[][][] dst = new int[2][][];
        System.arraycopy(cube, 0, dst, 0, 2);
        System.out.println(dst[1][2][3]);

        // 7. null row: loading through it NPEs, storing into a null leaf NPEs
        int[][][] nul = new int[2][2][2];
        nul[0] = null;
        try {
            System.out.println(nul[0][0][0]);
        } catch (NullPointerException e) {
            System.out.println("row NPE");
        }

        // 8. instanceof at depth
        Object o = cube;
        System.out.println((o instanceof int[][][]) + " " + (o instanceof Object[]));
        Object o2 = new String[1][1];
        System.out.println(o2 instanceof String[][]);

        // 9. the aastore component check: a String[][] cannot hold an int[]
        try {
            Object[] holder = new String[2][];
            holder[0] = cube[0];
            System.out.println("stored (wrong)");
        } catch (ArrayStoreException e) {
            System.out.println("ASE: " + e.getMessage());
        }

        // 10. Arrays.toString on a leaf; getClass().getName() shapes
        int[] leaf = cube[1][2];
        System.out.println(java.util.Arrays.toString(leaf));
        System.out.println(cube.getClass().getName());
        System.out.println(cube[0].getClass().getName());
        System.out.println(cube[0][0].getClass().getName());

        // 11. the aastore ASE message shapes (AseProbe on Temurin 21.0.13):
        //     the STORED object's external name ALONE — dotted plain
        //     classes, binary name keeps $, primitive arrays descriptor-
        //     verbatim (`[[I`)
        try {
            Object[] h = new String[1];
            h[0] = new Object();
        } catch (ArrayStoreException e) {
            System.out.println("ase1: " + e.getMessage());
        }
        try {
            Object[] h = new String[2][];
            h[0] = new int[3][3];
        } catch (ArrayStoreException e) {
            System.out.println("ase2: " + e.getMessage());
        }
        try {
            Object[] h = new int[2][];
            h[0] = "x";
        } catch (ArrayStoreException e) {
            System.out.println("ase3: " + e.getMessage());
        }
        try {
            Object[] h = new Integer[1];
            h[0] = Long.valueOf(3);
        } catch (ArrayStoreException e) {
            System.out.println("ase4: " + e.getMessage());
        }

        // 12. depth 4 (the depth-4 wave, `[[[[I`): full-dim allocation +
        //     nested length at every level
        int[][][][] hyper = new int[2][3][4][5];
        System.out.println(hyper.length + " " + hyper[0].length + " "
                + hyper[0][0].length + " " + hyper[0][0][0].length);

        // 13. nested store/load at depth 4: the leaf is an int
        hyper[1][2][3][4] = 42;
        hyper[0][0][0][0] = hyper[1][2][3][4] - 40;
        System.out.println(hyper[1][2][3][4] + " " + hyper[0][0][0][0]);

        // 14. partial allocation at depth 4: two dims specified, leaf
        //     matrices null until assigned
        int[][][][] hp = new int[2][3][][];
        System.out.println(hp.length + " " + hp[0].length + " " + (hp[1][2] == null));
        hp[1][2] = new int[4][5];
        hp[1][2][3][4] = 7;
        System.out.println(hp[1][2][3][4]);

        // 15. one-dim-only allocation at depth 4: every row null
        int[][][][] tops4 = new int[2][][][];
        System.out.println(tops4.length + " " + (tops4[1] == null));
        tops4[1] = new int[1][1][1];
        tops4[1][0][0][0] = 6;
        System.out.println(tops4[1][0][0][0]);

        // 16. the typed-lattice payoff: hyper[0] is a `[[[I`-shaped value
        //     and satisfies an int[][][]-typed call site
        System.out.println(probe3d(hyper[0]));

        // 17. clone() at depth 4: outer clones, everything below shares
        int[][][][] o4 = new int[1][1][1][1];
        o4[0][0][0][0] = 9;
        int[][][][] c4 = o4.clone();
        c4[0][0][0][0] = 11;
        System.out.println(o4[0][0][0][0] + " " + (c4 != o4) + " " + (c4[0] == o4[0]));

        // 18. arraycopy of a 4D array (reference semantics per element)
        int[][][][] d4 = new int[2][][][];
        System.arraycopy(hyper, 0, d4, 0, 2);
        System.out.println(d4[1][2][3][4]);

        // 19. null row at depth 4: loading through it NPEs
        int[][][][] n4 = new int[2][2][2][2];
        n4[1] = null;
        try {
            System.out.println(n4[1][0][0][0]);
        } catch (NullPointerException e) {
            System.out.println("row4 NPE");
        }

        // 20. instanceof at depth 4 + getClass().getName() shapes
        Object oo = hyper;
        System.out.println((oo instanceof int[][][][]) + " " + (oo instanceof Object[]));
        System.out.println(hyper.getClass().getName());
        System.out.println(hyper[0].getClass().getName());
        System.out.println(hyper[0][0].getClass().getName());

        // 21. the aastore ASE message at depth 4: a `[[[F` stored into an
        //     `[[[I` slot names the STORED object descriptor-verbatim
        try {
            Object[] h4 = new int[1][1][1][];
            h4[0] = new float[1][1][1];
        } catch (ArrayStoreException e) {
            System.out.println("ase5: " + e.getMessage());
        }
    }

    static int probe3d(int[][][] m) {
        return m[0][0][0];
    }
}