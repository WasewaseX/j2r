/**
 * NpeShapeProbe2 — the HELPFUL-NPE residual's oracle matrix (extends the
 * registered NpeShapeProbe, g3.3 slice 2, 2026-11-14): Temurin 21.0.13+9
 * carries the JEP 358 helpful NullPointerException messages; j2r throws
 * the bare NPE (message None) at every null-receiver law. Every shape is
 * CAUGHT and printed so the oracle is deterministic stdout — an uncaught
 * shape would diverge on stderr bytes instead. The probe pins WHERE the
 * helpful messages differ, per shape, so the fix slice can take them one
 * at a time. NOT a suite fixture until the fix lands (three-channel would
 * FAIL by design — the registered-residual precedent).
 */
public class NpeShapeProbe2 {

    static Double sf; // the static-field shape

    Double ifield;   // the instance-field shape

    static Double getS() { return null; }
    static String getS2() { return null; }

    static int[] getArr() { return null; }

    public static void main(String[] args) {
        // S1: the registered shape — unbox an absent-key get (call-result).
        try {
            java.util.Map<String, Double> m = new java.util.HashMap<>();
            double v = m.get("missing");
            System.out.println("S1 unreachable " + v);
        } catch (NullPointerException e) {
            System.out.println("S1 caught: msg=[" + e.getMessage() + "]");
        }

        // S2: unbox a NULL LOCAL (the local-variable-name shape).
        try {
            Double d = null;
            double v = d;
            System.out.println("S2 unreachable " + v);
        } catch (NullPointerException e) {
            System.out.println("S2 caught: msg=[" + e.getMessage() + "]");
        }

        // S3: explicit instance call on a null local.
        try {
            String s = null;
            int len = s.length();
            System.out.println("S3 unreachable " + len);
        } catch (NullPointerException e) {
            System.out.println("S3 caught: msg=[" + e.getMessage() + "]");
        }

        // S4: instance call on a CALL RESULT (the producer chain shape).
        try {
            int len = getS2().length();
            System.out.println("S4 unreachable " + len);
        } catch (NullPointerException e) {
            System.out.println("S4 caught: msg=[" + e.getMessage() + "]");
        }

        // S5: the array-length shape.
        try {
            int[] a = null;
            int n = a.length;
            System.out.println("S5 unreachable " + n);
        } catch (NullPointerException e) {
            System.out.println("S5 caught: msg=[" + e.getMessage() + "]");
        }

        // S6: the instance-field read shape.
        try {
            NpeShapeProbe2 p = new NpeShapeProbe2();
            NpeShapeProbe2 q = null;
            double v = q.ifield;
            System.out.println("S6 unreachable " + v);
        } catch (NullPointerException e) {
            System.out.println("S6 caught: msg=[" + e.getMessage() + "]");
        }

        // S7: the array-store-into-null-array shape.
        try {
            int[] a = null;
            a[0] = 1;
            System.out.println("S7 unreachable");
        } catch (NullPointerException e) {
            System.out.println("S7 caught: msg=[" + e.getMessage() + "]");
        }

        // S8: the boxed-Boolean unbox variant (different unbox method).
        try {
            java.util.Map<String, Boolean> m = new java.util.HashMap<>();
            boolean b = m.get("missing");
            System.out.println("S8 unreachable " + b);
        } catch (NullPointerException e) {
            System.out.println("S8 caught: msg=[" + e.getMessage() + "]");
        }

        System.out.println("done");
    }
}
