import java.util.*;

/** G3.3 slice 3 probe — the field/materialization layer.
 *
 * Pins (javac 21.0.13 shapes via javap, Temurin 21.0.13 laws via run):
 *   P1  <clinit>: `New HashMap; Dup; <init>; PutStatic M` — the creation
 *       run that lands in &lt;clinit&gt; for a static Map field.
 *   P2  put through the field: `GetStatic M; Ldc key; Ldc2W v; valueOf;
 *       InvokeInterface put; Pop` — the getstatic put core.
 *   P3  get through the field: `GetStatic M; Ldc key; InvokeInterface
 *       get; CheckCast Double; doubleValue` — the getstatic get core.
 *   P4  generic exit: `GetStatic M; Ldc key; InvokeInterface get;
 *       AReturn` — NO core (no checkcast/unbox adjacency): the faithful
 *       path the runtime materialization must serve.
 *   L1  get-twice-returns-the-same-ref through generic exits: true.
 *   L2  the JLS Integer identity cache survives through the map: the box
 *       a generic exit returns IS Integer.valueOf(5) for cached values.
 *   L3  entry.getValue() through iteration sees the same refs.
 */
public class Probe3 {
    static Map<String, Double> M = new HashMap<>();
    static Map<String, Integer> CACHE = new HashMap<>();

    static void seed() {
        M.put("a", 0.5);
        M.put("b", 1.5);
    }

    static double coreGet(String k) {
        return M.get(k);
    }

    static Object genericExit(String k) {
        return M.get(k);
    }

    public static void main(String[] args) {
        seed();
        System.out.println(coreGet("a"));
        Object o1 = genericExit("a");
        Object o2 = genericExit("a");
        System.out.println(o1);
        System.out.println(o1 == o2);
        System.out.println(o1 == genericExit("a"));
        CACHE.put("n", 5);
        Object c = CACHE.get("n");
        System.out.println(c == Integer.valueOf(5));
        double s = 0.0;
        for (Map.Entry<String, Double> e : M.entrySet()) {
            s += e.getValue();
        }
        System.out.println(s);
        System.out.println(M);
    }
}
