public class AcpyProbe {
    static void t(String name, Runnable r) {
        try {
            r.run();
            System.out.println(name + ": ok");
        } catch (Throwable e) {
            System.out.println(name + ": " + e.getClass().getName() + " | " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        int[] a = { 1, 2, 3, 4, 5 };
        int[] b = new int[5];
        System.arraycopy(a, 0, b, 0, 5);
        System.out.println("happy: " + java.util.Arrays.toString(b));
        // overlapping forward (src < dst, same array)
        int[] c = { 1, 2, 3, 4, 5 };
        System.arraycopy(c, 0, c, 1, 4);
        System.out.println("overlap-fwd: " + java.util.Arrays.toString(c));
        // overlapping backward (src > dst, same array)
        int[] d = { 1, 2, 3, 4, 5 };
        System.arraycopy(d, 1, d, 0, 4);
        System.out.println("overlap-bwd: " + java.util.Arrays.toString(d));
        // ref arrays with subtype check
        String[] s1 = { "x", "y" };
        Object[] o1 = new Object[2];
        System.arraycopy(s1, 0, o1, 0, 2);
        System.out.println("ref-upcast: " + o1[0] + "," + o1[1]);
        String[] s2 = new String[2];
        try { System.arraycopy(o1, 0, s2, 0, 2); } catch (Throwable e) {
            System.out.println("ref-downcast: " + e.getClass().getName() + " | " + e.getMessage());
        }
        // primitive mismatch
        t("prim-mismatch", () -> System.arraycopy(a, 0, new long[5], 0, 2));
        t("prim-to-ref", () -> System.arraycopy(a, 0, new Object[5], 0, 2));
        t("null-src", () -> System.arraycopy(null, 0, b, 0, 1));
        t("null-dst", () -> System.arraycopy(a, 0, null, 0, 1));
        t("neg-len", () -> System.arraycopy(a, 0, b, 0, -1));
        t("src-oob", () -> System.arraycopy(a, 3, b, 0, 5));
        t("dst-oob", () -> System.arraycopy(a, 0, b, 3, 5));
        t("neg-srcpos", () -> System.arraycopy(a, -1, b, 0, 2));
        t("neg-dstpos", () -> System.arraycopy(a, 0, b, -1, 2));
        t("zero-len-oob-pos", () -> System.arraycopy(a, 5, b, 0, 0));
        // char[] to int[]? both "IntGroup" JVM kinds but different Java types
        t("char-to-int", () -> System.arraycopy(new char[]{'a'}, 0, new int[1], 0, 1));
    }
}
