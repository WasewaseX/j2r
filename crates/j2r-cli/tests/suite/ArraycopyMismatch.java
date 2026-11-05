public class ArraycopyMismatch {
    static void t(String name, Runnable r) {
        try {
            r.run();
            System.out.println(name + ": ok");
        } catch (Throwable e) {
            System.out.println(name + ": " + e.getClass().getName() + " | " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        // A: ref→ref with all elements passing the per-element check —
        // NO upfront component rejection on Temurin (Object[] → String[])
        Object[] oa = { "a", "b", "c" };
        String[] sa = new String[3];
        t("A-object-to-string", () -> System.arraycopy(oa, 0, sa, 0, 3));
        System.out.println("A-dst-after: " + java.util.Arrays.toString(sa));
        // B: mid-copy failure at element 1 — partial copy stays visible
        Object[] bad = { "a", Integer.valueOf(1), "c" };
        String[] dstB = new String[3];
        t("B-mid-fail", () -> System.arraycopy(bad, 0, dstB, 0, 3));
        System.out.println("B-dst-after: " + java.util.Arrays.toString(dstB));
        // C: failure at element 0 — destination untouched
        Object[] bad0 = { Integer.valueOf(1), "b", "c" };
        String[] dstC = new String[3];
        t("C-fail-at-0", () -> System.arraycopy(bad0, 0, dstC, 0, 3));
        System.out.println("C-dst-after: " + java.util.Arrays.toString(dstC));
        // D: interface component down (Comparable → Integer), mid-copy failure
        Comparable[] comps = { Integer.valueOf(1), "x", Integer.valueOf(3) };
        Integer[] ints = new Integer[3];
        t("D-comparable-to-integer", () -> System.arraycopy(comps, 0, ints, 0, 3));
        System.out.println("D-dst-after: " + java.util.Arrays.toString(ints));
        // E: null elements copy fine into a reference destination
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
        System.out.println("G-dst-after: " + java.util.Arrays.toString(ok2));
        // H: nested arrays — the array-shaped names in the mismatch message
        Object[][] rows = { new String[1], new Integer[1] };
        String[][] ssm = new String[2][];
        t("H-nested-dst", () -> System.arraycopy(rows, 0, ssm, 0, 2));
        Integer[][] isrc = { new Integer[1], null };
        Object[][] odst = new Object[2][];
        t("H-nested-src-ok", () -> System.arraycopy(isrc, 0, odst, 0, 2));
        System.out.println("H-nested-src-nonnull: " + (odst[0] != null));
        String[][] ssrc = { new String[1] };
        Object[][] odst2 = new Object[1][];
        t("H-nested-upcast", () -> System.arraycopy(ssrc, 0, odst2, 0, 1));
        System.out.println("H-nested-upcast-nonnull: " + (odst2[0] != null));
        // I: the pinned upfront messages (prim↔ref, kinds) — regression
        t("I-prim-to-ref", () -> System.arraycopy(new double[]{1.0}, 0, new Object[1], 0, 1));
        t("I-ref-to-prim", () -> System.arraycopy(new Object[1], 0, new double[1], 0, 1));
        t("I-refprim", () -> System.arraycopy(new String[1], 0, new int[1], 0, 1));
        System.out.println("done");
    }
}
