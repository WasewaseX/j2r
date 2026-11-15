import java.util.TreeMap;
import java.util.LinkedHashMap;
import java.util.Map;

public class TreeMapLhmPrimProbe {
    static TreeMap<String, Long> STM = new TreeMap<>();
    static LinkedHashMap<String, Integer> SLM = new LinkedHashMap<>();

    public static void main(String[] args) {
        // ---- A. TreeMap locals, Double values (the put/get core shapes) ----

        // c01. core puts then core gets: the rewritten path computes the same sums
        TreeMap<String, Double> t = new TreeMap<>();
        t.put("b", 1.5); t.put("a", 2.5); t.put("c", 3.5);
        double s = t.get("a") + t.get("b");
        System.out.println("c01: s=" + s);

        // c02. sorted order preserved with raw prim slots (toString materializes)
        System.out.println("c02: " + t);

        // c03. core get-miss -> NPE (the class only; the message is the
        // registered HELPFUL-NPE residual, same-surface law)
        try { double d = t.get("no"); System.out.println("c03: NO-THROW " + d); }
        catch (NullPointerException e) { System.out.println("c03: NPE"); }

        // c04. L1: get-twice-returns-the-same-ref through the generic exit
        System.out.println("c04: " + (t.get("a") == t.get("a")));

        // c05. L2 for Double: valueOf never caches, so identity is FALSE
        System.out.println("c05: " + (t.get("a") == Double.valueOf(2.5)));

        // c06. entrySet iteration materializes values in sorted order
        StringBuilder sb = new StringBuilder();
        for (Map.Entry<String, Double> e : t.entrySet())
            sb.append(e.getKey()).append('=').append(e.getValue()).append(';');
        System.out.println("c06: " + sb);

        // c07. faithful overwrite (the return is USED, no Pop -> no core):
        // the old value materializes out of the raw prim slot
        Object old = t.put("a", 9.5);
        System.out.println("c07: old=" + old + " map=" + t);

        // c08. core get after a faithful overwrite (a real box sits in the slot)
        System.out.println("c08: " + t.get("a"));

        // c09. faithful remove returns the materialized old value
        t.put("d", 4.25);
        Object ro = t.remove("d");
        System.out.println("c09: removed=" + ro + " has=" + t.containsKey("d"));

        // c10. containsValue with raw prim slots (the all-entries reader)
        System.out.println("c10: " + t.containsValue(3.5) + " " + t.containsValue(99.0));

        // c11. getOrDefault materializes / misses to the default
        System.out.println("c11: " + t.getOrDefault("c", -1.0) + " "
            + t.getOrDefault("zz", -1.0));

        // c12. CCE on a non-Comparable key through the ORDER compare —
        // the self-compare (compare(key,key), the addEntryToEmptyMap law)
        // throws on the FIRST put into an empty natural-order map
        // (StringBuilder would NOT pin it: Comparable since JDK 11)
        TreeMap<Object, Double> bad = new TreeMap<>();
        try { bad.put(new Object(), 1.0); System.out.println("c12: NO-THROW"); }
        catch (ClassCastException e) { System.out.println("c12: CCE"); }

        // c13. Map-typed local: the invoke class is java/util/Map, the
        // creation run is still TreeMap-fresh
        Map<String, Double> m = new TreeMap<>();
        m.put("k", 7.75);
        System.out.println("c13: " + m.get("k") + " " + m);

        // c14. Long core path + arithmetic on the unboxed result
        TreeMap<String, Long> tl = new TreeMap<>();
        tl.put("x", 10L); tl.put("y", 20L);
        System.out.println("c14: " + (tl.get("x") + tl.get("y")) + " " + tl);

        // c15. Integer-KEY TreeMap: the key's valueOf stays OUTSIDE the
        // cores (only the value adjacency matches) — keys stay boxed and
        // the Integer ordering law carries
        TreeMap<Integer, Double> ti = new TreeMap<>();
        ti.put(3, 0.5); ti.put(1, 1.5); ti.put(2, 2.5);
        System.out.println("c15: " + ti + " g=" + ti.get(1));

        // ---- B. SubMap views ride the raw slots ----

        // c16. view toString + view get on a prim-tagged backing map
        TreeMap<String, Double> tv = new TreeMap<>();
        tv.put("a", 1.0); tv.put("b", 2.0); tv.put("m", 3.0); tv.put("z", 4.0);
        System.out.println("c16: " + tv.subMap("a", "m")
            + " get=" + tv.subMap("a", "m").get("b"));

        // c17. view iteration materializes (the bounded iterators)
        StringBuilder vb = new StringBuilder();
        for (Map.Entry<String, Double> e : tv.subMap("a", "m").entrySet())
            vb.append(e.getValue()).append(';');
        System.out.println("c17: " + vb);

        // c18. view getOrDefault / containsValue readers
        System.out.println("c18: " + tv.subMap("a", "m").containsValue(2.0)
            + " " + tv.subMap("a", "m").getOrDefault("z", -7.0));

        // ---- C. LinkedHashMap locals, Integer values ----

        // c19. core puts keep INSERTION order with raw prim slots
        LinkedHashMap<String, Integer> l = new LinkedHashMap<>();
        l.put("b", 5); l.put("a", 3); l.put("c", 200);
        System.out.println("c19: " + l);

        // c20. core gets + arithmetic
        System.out.println("c20: " + (l.get("a") + l.get("b")));

        // c21. core get-miss -> NPE (class only)
        try { int q = l.get("no"); System.out.println("c21: NO-THROW " + q); }
        catch (NullPointerException e) { System.out.println("c21: NPE"); }

        // c22. L1 through the link-chain storage
        System.out.println("c22: " + (l.get("a") == l.get("a")));

        // c23. Integer cache law: within the cache -> valueOf identity TRUE
        System.out.println("c23: " + (l.get("a") == Integer.valueOf(3)));

        // c24. beyond the cache: identity FALSE; iteration keeps insertion order
        StringBuilder lb = new StringBuilder();
        for (Map.Entry<String, Integer> e : l.entrySet())
            lb.append(e.getKey()).append('=').append(e.getValue()).append(';');
        System.out.println("c24: " + lb + " eq200=" + (l.get("c") == Integer.valueOf(200)));

        // c25. faithful overwrite return + core get after (the box lands)
        Object lo = l.put("b", 50);
        System.out.println("c25: old=" + lo + " now=" + l.get("b") + " map=" + l);

        // c26. remove return materializes; the link chain survives the unlink
        Object lro = l.remove("c");
        System.out.println("c26: removed=" + lro + " size=" + l.size() + " map=" + l);

        // c27. Map-typed LHM local
        Map<String, Integer> lm = new LinkedHashMap<>();
        lm.put("n", 9);
        System.out.println("c27: " + lm.get("n") + " " + lm);

        // ---- D. static fields (the proven-field layer rides both maps) ----

        // c28. STM core puts + core gets through the field receiver
        STM.put("u", 100L); STM.put("v", 200L);
        System.out.println("c28: " + (STM.get("u") + STM.get("v")) + " " + STM);

        // c29. SLM core puts + the cache identity through the field
        SLM.put("p", 7); SLM.put("q", 8);
        System.out.println("c29: " + (SLM.get("p") + SLM.get("q"))
            + " eq=" + (SLM.get("p") == Integer.valueOf(7)));

        // c30. static-field generic exits: toString + insertion order
        System.out.println("c30: " + SLM);

        // c31. faithful put through the static field returns materialized old
        Object so = STM.put("u", 111L);
        System.out.println("c31: old=" + so + " now=" + STM.get("u"));

        // c32. containsValue / getOrDefault on the static fields
        System.out.println("c32: " + STM.containsValue(200L)
            + " " + SLM.getOrDefault("p", -1));

        // ---- E. mixed faithful/core interleaving on one map ----

        // c33. three core puts on ONE key: overwrite through raw slots
        TreeMap<String, Double> tx = new TreeMap<>();
        tx.put("r", 1.0);
        tx.put("r", 2.0);
        tx.put("r", 3.0);
        System.out.println("c33: " + tx.get("r") + " " + tx);

        // c34. faithful NULL value onto a fresh key, then core get -> NPE
        TreeMap<String, Double> tn = new TreeMap<>();
        tn.put("z", 1.0);
        tn.put("y", null);
        try { double d = tn.get("y"); System.out.println("c34: NO-THROW " + d); }
        catch (NullPointerException e) { System.out.println("c34: NPE"); }
        System.out.println("c34b: " + tn);

        // c35. faithful NULL over an EXISTING prim-tagged slot: the tag is
        // an annotation, not a mode switch — the null passes through
        tn.put("z", null);
        try { double d = tn.get("z"); System.out.println("c35: NO-THROW " + d); }
        catch (NullPointerException e) { System.out.println("c35: NPE"); }
        System.out.println("c35b: " + tn);
    }
}
