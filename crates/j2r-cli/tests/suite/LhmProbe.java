import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;

public class LhmProbe {
    public static void main(String[] args) {
        // L1: insertion order
        LinkedHashMap<String, Integer> m = new LinkedHashMap<>();
        m.put("a", 1);
        m.put("b", 2);
        m.put("c", 3);
        System.out.println(m);
        // L2: re-put existing key — value replaced, position kept
        m.put("a", 10);
        System.out.println(m);
        // L3: remove + re-put — moves to tail
        m.remove("a");
        m.put("a", 11);
        System.out.println(m);
        // L4: get / containsKey do not reorder
        m.get("b");
        m.containsKey("c");
        System.out.println(m);
        // L5: ctor-from-map copies in source iteration order
        LinkedHashMap<String, Integer> copy = new LinkedHashMap<>(m);
        System.out.println(copy);
        HashMap<String, Integer> hm = new HashMap<>();
        hm.put("x", 1);
        hm.put("y", 2);
        hm.put("z", 3);
        LinkedHashMap<String, Integer> fromHm = new LinkedHashMap<>(hm);
        System.out.println(fromHm);
        // L6: is-a laws
        System.out.println(m instanceof LinkedHashMap);
        System.out.println(m instanceof HashMap);
        Map<String, Integer> asMap = m;
        System.out.println(asMap.size());
        // L7: the three views in link order
        StringBuilder ks = new StringBuilder();
        for (String k : copy.keySet()) {
            ks.append(k).append(";");
        }
        System.out.println(ks);
        StringBuilder vs = new StringBuilder();
        for (int v : copy.values()) {
            vs.append(v).append(";");
        }
        System.out.println(vs);
        StringBuilder es = new StringBuilder();
        for (Map.Entry<String, Integer> e : copy.entrySet()) {
            es.append(e.getKey()).append("=").append(e.getValue()).append(";");
        }
        System.out.println(es);
        // L8: iterator remove removes the last-returned element, order intact
        Iterator<Map.Entry<String, Integer>> it = copy.entrySet().iterator();
        it.next();
        it.remove();
        System.out.println(copy);
        System.out.println(copy.size());
        // L9: null key (one), null value
        m.put(null, 0);
        System.out.println(m);
        m.put("n", null);
        System.out.println(m);
        m.remove(null);
        System.out.println(m);
        // L10: clear + re-add
        copy.clear();
        System.out.println(copy.isEmpty());
        copy.put("q", 7);
        System.out.println(copy);
        // L11: getOrDefault / containsValue / containsKey
        System.out.println(m.getOrDefault("b", -1));
        System.out.println(m.getOrDefault("missing", -1));
        System.out.println(m.containsValue(11));
        System.out.println(m.containsValue(999));
        System.out.println(m.get("missing"));
    }
}
