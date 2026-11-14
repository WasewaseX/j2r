import java.util.HashMap;
import java.util.Map;

/**
 * G3.3 slice 2 — the primitive-specialized map path (the 2.78x container
 * tax, register step 3). Every access on each map local is javac's
 * autobox core shape — `valueOf -> put -> pop` and `get -> checkcast ->
 * xValue()` — on a HashMap-fresh, consumption-closed receiver local, so
 * the specialized-map rewrite fires and the node slots hold RAW prim
 * payloads. The differential proves the prim-slot representation is
 * unobservable: the output is byte-identical to Temurin's boxing path on
 * all three channels (stdout/stderr/exit), including the Integer/Long
 * [-128,127] cache law (under the closure law the box never materializes
 * either way) and the Float/Double never-cached pair.
 *
 * The faithful path is exercised by every other map fixture (e.g.
 * HashMapOrder, whose locals fail the closure law on purpose); the
 * rewrite's own refusal rules are unit-tested in the peephole.
 */
public class MapPrim {
    public static void main(String[] args) {
        // Double map: seed put (const), get+add, put (arith), final get.
        Map<String, Double> m = new HashMap<String, Double>();
        m.put("a", 0.5);
        double x = m.get("a") + 1.0;
        m.put("a", x * 2.0);
        double aOut = m.get("a");

        // The BoxingBoxedBench aggregation shape, shrunk: the hot loop is
        // get -> xValue -> +w -> valueOf -> put per step — the two boxed
        // ops the rewrite collapses to one native call each.
        Map<String, Double> w = new HashMap<String, Double>();
        for (int i = 0; i < 4; i++) {
            w.put("k" + i, 0.25 * i);
        }
        for (int r = 0; r < 1000; r++) {
            for (int k = 0; k < 4; k++) {
                w.put("k" + k, w.get("k" + k) + 0.001);
            }
        }
        double chk = 0.0;
        for (int k = 0; k < 4; k++) {
            chk += w.get("k" + k);
        }

        // Integer map: the JLS cache range (42) — the rewritten path
        // stores the raw Int payload; a faithful reader could never tell.
        Map<String, Integer> mi = new HashMap<String, Integer>();
        mi.put("n", 42);
        int iv = mi.get("n") * 2;
        mi.put("n", iv + 1);
        int nOut = mi.get("n");

        // Long map: category-2 payload both directions.
        Map<String, Long> ml = new HashMap<String, Long>();
        ml.put("L", 123456789012345L);
        long lv = ml.get("L") + 1L;
        ml.put("L", lv);
        long lOut = ml.get("L");

        // Float map: the never-cached pair's other half.
        Map<String, Float> mf = new HashMap<String, Float>();
        mf.put("f", 1.5f);
        float fv = mf.get("f") * 2.0f;
        mf.put("f", fv);
        float fOut = mf.get("f");

        // Values print through typed locals — a println(map.get(...))
        // would pass the box as Object (println(Object)) and refuse the
        // closure law, which is exactly the registered design.
        System.out.println(x);
        System.out.println(aOut);
        System.out.println(chk);
        System.out.println(nOut);
        System.out.println(lOut);
        System.out.println(fOut);
    }
}
