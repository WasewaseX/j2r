/**
 * Temurin vector for deep primitive-array descriptors (`[[[I` and deeper —
 * the registered "`[[[<prim>` verifier depth" gap): allocation with partial
 * dims, element store/load across levels, length at each level, clone(),
 * System.arraycopy of 3D arrays, null-row NPEs, instanceof, and the
 * aastore component check on deep arrays.
 */
public class DeepArraysProbe {

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
    }
}
