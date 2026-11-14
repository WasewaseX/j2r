import java.util.HashMap;
import java.util.Map;

/**
 * G3.3 slice 3 — the field/materialization layer. Every map here is a
 * STATIC FIELD whose every store in &lt;clinit&gt; is javac's creation run
 * (`new HashMap; dup; <init>; putstatic`), so the class-level scan proves
 * the provenance law and the put/get cores through `getstatic M` collapse
 * to the prim-slot natives exactly like slice 2's local maps. The slots
 * then hold RAW prim payloads stamped with the node's class tag.
 *
 * The faithful exits are exercised DIFFERENTIALLY on those prim slots:
 * the generic exits (`Object o = M.get(k)`) materialize the box lazily
 * and cache it in the node slot — get-twice-returns-the-same-ref stays
 * TRUE (three generic exits of one key compare identical), and the JLS
 * identity cache survives through the map (the materialized box IS
 * `Integer.valueOf(5)`'s shared cache box, while `Integer.valueOf(1000)`
 * — outside [-128,127] — is a fresh object every time). entry.getValue()
 * through iteration and `toString` materialize every slot they print.
 *
 * Byte-identical vs Temurin 21.0.13 on all three channels.
 */
public class MapField {
    static Map<String, Double> M = new HashMap<String, Double>();
    static Map<String, Integer> CACHE = new HashMap<String, Integer>();
    static Map<String, Long> ACC = new HashMap<String, Long>();

    static void seed() {
        M.put("a", 0.5);
        M.put("b", 1.5);
    }

    static double coreGet(String k) {
        return M.get(k);
    }

    static void bump(String k, double w) {
        M.put(k, M.get(k) + w);
    }

    static Object genericExit(String k) {
        return M.get(k);
    }

    public static void main(String[] args) {
        seed();
        // The rewritten cores through the field.
        System.out.println(coreGet("a"));
        bump("a", 0.25);
        System.out.println(coreGet("a"));

        // The generic exits materialize the box; get-twice-returns-the-
        // same-ref (three reads of one slot compare identical).
        Object o1 = genericExit("a");
        Object o2 = genericExit("a");
        System.out.println(o1);
        System.out.println(o1 == o2);
        System.out.println(o1 == genericExit("a"));

        // The JLS identity cache through the map: the materialized box IS
        // the valueOf cache box inside [-128,127], a fresh object outside.
        CACHE.put("n", 5);
        CACHE.put("big", 1000);
        Object c = CACHE.get("n");
        Object big = CACHE.get("big");
        System.out.println(c == Integer.valueOf(5));
        System.out.println(big == Integer.valueOf(1000));
        System.out.println(c);
        System.out.println(big);

        // Long accumulation through the rewritten cores (category-2 both
        // directions, value far outside the cache range).
        ACC.put("t", 123456789012345L);
        for (int i = 0; i < 3; i++) {
            ACC.put("t", ACC.get("t") + 1000000000000L);
        }
        System.out.println(ACC.get("t"));
        Object l1 = genericExit2(ACC, "t");
        Object l2 = genericExit2(ACC, "t");
        System.out.println(l1 == l2);

        // entry.getValue() through iteration materializes (the registered
        // design's named generic exit).
        double s = 0.0;
        for (Map.Entry<String, Double> e : M.entrySet()) {
            s += e.getValue();
        }
        System.out.println(s);

        // toString materializes every slot it prints.
        System.out.println(M);
        System.out.println(CACHE);
    }

    static Object genericExit2(Map<String, Long> m, String k) {
        return m.get(k);
    }
}
