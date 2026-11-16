public class NpeShapes3 {
    static Object getObj() { return null; }
    int ifld;
    static int[] iArr;
    public static void main(String[] args) {
        // A1: iaload (int array load)
        try { int[] a = null; int v = a[0]; System.out.println("A1 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A1: msg=[" + m(e) + "]"); }
        // A2: aaload (object array load)
        try { Object[] a = null; Object v = a[0]; System.out.println("A2 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A2: msg=[" + m(e) + "]"); }
        // A3: baload on byte[]
        try { byte[] a = null; byte v = a[0]; System.out.println("A3 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A3: msg=[" + m(e) + "]"); }
        // A4: laload
        try { long[] a = null; long v = a[0]; System.out.println("A4 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A4: msg=[" + m(e) + "]"); }
        // A5: aastore
        try { Object[] a = null; a[0] = "x"; System.out.println("A5 unreachable"); }
        catch (NullPointerException e) { System.out.println("A5: msg=[" + m(e) + "]"); }
        // A6: castore
        try { char[] a = null; a[0] = 'x'; System.out.println("A6 unreachable"); }
        catch (NullPointerException e) { System.out.println("A6: msg=[" + m(e) + "]"); }
        // A7: putfield (instance field write on null receiver)
        try { NpeShapes3 p = new NpeShapes3(); NpeShapes3 q = null; q.ifld = 1; System.out.println("A7 unreachable"); }
        catch (NullPointerException e) { System.out.println("A7: msg=[" + m(e) + "]"); }
        // A8: monitorenter
        try { Object o = null; synchronized (o) { System.out.println("A8 unreachable"); } }
        catch (NullPointerException e) { System.out.println("A8: msg=[" + m(e) + "]"); }
        // A9: athrow on null
        try { RuntimeException r = null; throw r; }
        catch (NullPointerException e) { System.out.println("A9: msg=[" + m(e) + "]"); }
        // A10: array-arg method (the arg-list grammar for array params)
        try { String s = null; char[] c = new char[4]; s.getChars(0, 1, c, 0); System.out.println("A10 unreachable"); }
        catch (NullPointerException e) { System.out.println("A10: msg=[" + m(e) + "]"); }
        // A11: producer chain 2 args: m.put(k, v) result unbox
        try { java.util.Map<String, Integer> m1 = new java.util.HashMap<>(); int v = m1.put("a", 1); System.out.println("A11 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A11: msg=[" + m(e) + "]"); }
        // A12: cause = getstatic (static field read producer)
        try { int v = iArr.length; System.out.println("A12 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A12: msg=[" + m(e) + "]"); }
        // A13: cause = getfield chain (q.ifield read then unbox on null field value)
        try { NpeShapes3 q = new NpeShapes3(); Double dd = null; q.ifld2 = dd; }
        catch (NullPointerException e) { System.out.println("A13: msg=[" + m(e) + "]"); }
        // A14: instance method (non-static) local slot with a param
        run14(null);
        // A15: written param slot → localN form
        run15();
        System.out.println("done");
    }
    static Double ifld2;
    static void run14(String p) {
        try { int n = p.length(); System.out.println("A14 unreachable " + n); }
        catch (NullPointerException e) { System.out.println("A14: msg=[" + m(e) + "]"); }
    }
    static void run15() {
        try { Integer d = null; d = (Integer) getObj(); int v = d; System.out.println("A15 unreachable " + v); }
        catch (NullPointerException e) { System.out.println("A15: msg=[" + m(e) + "]"); }
    }
    static String m(NullPointerException e) { return e.getMessage(); }
    static String e(Exception x) { return "unreachable"; }
}
