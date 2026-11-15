import java.util.TreeMap;
import java.util.Map;
import java.util.HashMap;
import java.util.Iterator;

public class TreeMapProbe {
    public static void main(String[] a) {
        // 1. THE CORE LAW: iteration in KEY order, not insertion/hash order
        TreeMap<Integer, String> t = new TreeMap<>();
        t.put(42, "a"); t.put(-7, "b"); t.put(0, "c"); t.put(100, "d"); t.put(13, "e");
        StringBuilder sb = new StringBuilder();
        for (Integer k : t.keySet()) sb.append(k).append(" ");
        System.out.println("case1-keys: " + sb);
        StringBuilder vb = new StringBuilder();
        for (String v : t.values()) vb.append(v).append(" ");
        System.out.println("case1-vals: " + vb);
        StringBuilder eb = new StringBuilder();
        for (Map.Entry<Integer, String> e : t.entrySet())
            eb.append(e.getKey()).append("=").append(e.getValue()).append(";");
        System.out.println("case1-entries: " + eb);
        System.out.println("case1-toString: " + t);

        // 2. firstKey / lastKey
        System.out.println("case2: first=" + t.firstKey() + " last=" + t.lastKey());

        // 3. duplicate put replaces, keeps position, returns old
        String old = t.put(13, "E13");
        System.out.println("case3: old=" + old + " get13=" + t.get(13) + " size=" + t.size());
        StringBuilder sb3 = new StringBuilder();
        for (Integer k : t.keySet()) sb3.append(k).append(" ");
        System.out.println("case3-order: " + sb3);

        // 4. remove preserves order, returns old, absent -> null
        String r = t.remove(0);
        String r2 = t.remove(999);
        System.out.println("case4: removed=" + r + " absent=" + r2 + " size=" + t.size());
        System.out.println("case4-order: " + t.keySet());

        // 5. ceiling/floor/higher/lower (exact match vs strict)
        System.out.println("case5: ceil13=" + t.ceilingKey(13) + " floor13=" + t.floorKey(13)
            + " ceil14=" + t.ceilingKey(14) + " floor14=" + t.floorKey(14)
            + " hi13=" + t.higherKey(13) + " lo13=" + t.lowerKey(13)
            + " ceil999=" + t.ceilingKey(999) + " floor-99=" + t.floorKey(-99));

        // 6. comparator surface: descending via lambda
        TreeMap<Integer, String> d = new TreeMap<>((x, y) -> y - x);
        d.put(42, "a"); d.put(-7, "b"); d.put(0, "c"); d.put(100, "d");
        System.out.println("case6-desc: " + d);
        System.out.println("case6-first=" + d.firstKey() + " last=" + d.lastKey());

        // 7. null key on natural ordering -> NPE (no compareTo)
        TreeMap<Integer, String> n = new TreeMap<>();
        try { n.put(null, "x"); System.out.println("case7: NO-THROW"); }
        catch (NullPointerException e) { System.out.println("case7: NPE"); }

        // 8. ctor-from-map: sorted regardless of source order
        HashMap<Integer, String> src = new HashMap<>();
        src.put(5, "v5"); src.put(-3, "v-3"); src.put(27, "v27");
        TreeMap<Integer, String> c = new TreeMap<>(src);
        System.out.println("case8: " + c);

        // 9. iterator remove mid-walk: order intact, no CME
        TreeMap<Integer, String> w = new TreeMap<>();
        w.put(1, "a"); w.put(2, "b"); w.put(3, "c"); w.put(4, "d");
        Iterator<Integer> it = w.keySet().iterator();
        StringBuilder sb9 = new StringBuilder();
        while (it.hasNext()) {
            Integer k = it.next();
            if (k == 2) it.remove();
            else sb9.append(k).append(" ");
        }
        System.out.println("case9: walked=" + sb9 + " size=" + w.size() + " has2=" + w.containsKey(2));

        // 10. CME: foreign modification during iteration
        TreeMap<Integer, String> m = new TreeMap<>();
        m.put(1, "a"); m.put(2, "b");
        Iterator<Integer> it2 = m.keySet().iterator();
        it2.next();
        m.put(3, "c");
        try { it2.next(); System.out.println("case10: NO-THROW"); }
        catch (java.util.ConcurrentModificationException e) { System.out.println("case10: CME"); }

        // 11. size/isEmpty/clear/getOrDefault/containsValue
        TreeMap<Integer, String> g = new TreeMap<>();
        System.out.println("case11: empty=" + g.isEmpty());
        g.put(7, "v"); g.put(8, "w");
        System.out.println("case11: size=" + g.size() + " gd=" + g.getOrDefault(9, "D")
            + " cv=" + g.containsValue("w") + " cf=" + g.containsKey(7));
        g.clear();
        System.out.println("case11: cleared=" + g.isEmpty() + " size=" + g.size());

        // 13. non-Comparable key -> ClassCastException (probed separately,
        // CceProbe vs Temurin: the bare CCE catch)
        TreeMap<Object, String> cce = new TreeMap<>();
        try { cce.put(new Object(), "x"); System.out.println("case13: NO-THROW"); }
        catch (ClassCastException e) { System.out.println("case13: CCE"); }
        System.out.println("done");
    }
}
