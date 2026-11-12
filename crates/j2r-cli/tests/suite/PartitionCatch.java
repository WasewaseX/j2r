import java.util.ArrayList;
/**
 * External-B2/O5 (jbox2d port report, 2026-11-13): partitioned emission
 * previously wired the exception-matcher helper to a module that does not
 * exist in a partition crate (E0433: cannot find `registry` in the crate
 * root) — jbox2d's World$1.&lt;clinit&gt; was the first hit, because every
 * partition fixture so far was try/catch-free. This fixture is the
 * exception-table partition law: try/catch + finally in a partitioned
 * class, a &lt;clinit&gt; that CATCHES an exception (the World$1 shape),
 * and cross-partition calls while the handlers resolve catch types
 * through the installed registry hook. Runs under --partition-classes 2
 * (partition_suite) AND the plain differential suite.
 */
public class PartitionCatch {

    static class Op {
        static int risky(int[] a, int i) {
            try {
                return a[i] / (i - i);
            } catch (ArithmeticException e) {
                return -1;
            } catch (ArrayIndexOutOfBoundsException e) {
                return -2;
            } finally {
                a[0] += 1;
            }
        }
    }

    static class Seed {
        static int MODE;
        static {
            try {
                MODE = 10 / 0;
            } catch (ArithmeticException e) {
                MODE = 7;
            }
        }
    }

    public static void main(String[] args) {
        int[] a = new int[3];
        ArrayList<String> log = new ArrayList<>();
        log.add("risky0=" + Op.risky(a, 0));
        log.add("risky5=" + Op.risky(a, 5));
        log.add("mode=" + Seed.MODE);
        log.add("a0=" + a[0]);
        try {
            Object o = null;
            o.toString();
            log.add("no npe");
        } catch (NullPointerException e) {
            log.add("npe caught");
        }
        System.out.println(log.get(0) + "," + log.get(1) + "," + log.get(2)
            + "," + log.get(3) + "," + log.get(4));
    }
}
