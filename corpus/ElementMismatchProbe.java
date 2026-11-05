public class ElementMismatchProbe {
    static void t(String name, Runnable r) {
        try {
            r.run();
            System.out.println(name + ": ok");
        } catch (Throwable e) {
            System.out.println(name + ": " + e.getClass().getName() + " | " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        // A: per-element pass (all elements Strings) — confirmed ok, no upfront check
        Object[] oa = { "a", "b", "c" };
        String[] sa = new String[3];
        t("A-object-to-string", () -> System.arraycopy(oa, 0, sa, 0, 3));
        System.out.println("A-dst-after: " + java.util.Arrays.toString(sa));
        // B: mid-copy failure at element 1 (Integer into String[])
        Object[] bad = { "a", Integer.valueOf(1), "c" };
        String[] dstB = new String[3];
        t("B-mid-fail", () -> System.arraycopy(bad, 0, dstB, 0, 3));
        System.out.println("B-dst-after: " + java.util.Arrays.toString(dstB));
        // C: failure at element 0 — dst must stay untouched
        Object[] bad0 = { Integer.valueOf(1), "b", "c" };
        String[] dstC = new String[3];
        t("C-fail-at-0", () -> System.arraycopy(bad0, 0, dstC, 0, 3));
        System.out.println("C-dst-after: " + java.util.Arrays.toString(dstC));
        // D: interface component down (Comparable -> Integer), failure mid-copy
        Comparable[] comps = { Integer.valueOf(1), "x", Integer.valueOf(3) };
        Integer[] ints = new Integer[3];
        t("D-comparable-to-integer", () -> System.arraycopy(comps, 0, ints, 0, 3));
        System.out.println("D-dst-after: " + java.util.Arrays.toString(ints));
        // E: null elements copy fine into ref dst
        Object[] withNull = { null, "x" };
        String[] dstE = new String[2];
        t("E-null-element", () -> System.arraycopy(withNull, 0, dstE, 0, 2));
        System.out.println("E-dst-after: " + java.util.Arrays.toString(dstE));
        // F: component upcasts all fine
        Integer[] i2 = { Integer.valueOf(6) };
        Number[] n2 = new Number[1];
        Comparable[] c2 = new Comparable[1];
        Object[] o2 = new Object[1];
        t("F-to-number", () -> System.arraycopy(i2, 0, n2, 0, 1));
        t("F-to-comparable", () -> System.arraycopy(i2, 0, c2, 0, 1));
        t("F-to-object", () -> System.arraycopy(i2, 0, o2, 0, 1));
        // G: same class, all fine (sanity)
        Integer[] ok = { Integer.valueOf(5) };
        Integer[] ok2 = new Integer[1];
        t("G-same-class", () -> System.arraycopy(ok, 0, ok2, 0, 1));
        // H: double[] to Object[] (prim to ref) — the pinned upfront message
        t("H-prim-to-ref", () -> System.arraycopy(new double[]{1.0}, 0, new Object[1], 0, 1));
        // I: Object[] to double[] (ref to prim)
        t("I-ref-to-prim", () -> System.arraycopy(new Object[1], 0, new double[1], 0, 1));
        // J: String[] to int[] (ref to prim, mismatched kinds)
        t("J-refprim", () -> System.arraycopy(new String[1], 0, new int[1], 0, 1));
    }
}
