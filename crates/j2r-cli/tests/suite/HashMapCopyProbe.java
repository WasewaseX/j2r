import java.util.*;

/**
 * HashMapCopyProbe — the copy-ctor probe for the registered HashMap
 * copy-ctor gap (new HashMap<>(map) is currently the loud unsupported
 * error). Pins the putMapEntries laws Temurin 21.0.13+9 exhibits:
 *
 * L1  the source-order insertion law read through CHAIN order: three
 *     Integer keys that collide into one bucket at every candidate
 *     capacity, inserted via a LinkedHashMap source in the order
 *     16, 0, 32 — the copy must iterate 16 0 32 (source link order),
 *     proving the source's iteration order drives insertion.
 * L2  the pre-size law at the cap-16/32 boundary: threshold =
 *     tableSizeFor((int)((float)s / 0.75f + 1.0f)) — s=11 gives
 *     15.67 -> 15 -> cap 16, s=12 gives 17.0 -> 17 -> cap 32. Key 20
 *     makes the difference OBSERVABLE: 20 & 15 = 4 collides with key 4
 *     at cap 16 (order ... 4 20 5 ...), while 20 & 31 = 20 is its own
 *     bucket at cap 32 (order ... 11 20).
 * L3  the hm-source law: a copy of a copy keeps the source's table
 *     order (same layout law, same cap).
 * L4  the tree-source law: a TreeMap source inserts in SORTED order;
 *     the copy's table order follows. L4b rides the same law through
 *     new LinkedHashMap<>(treeMap) — the LHM ctor must read the tree
 *     storage, not misread the hash slots.
 * L5  content equality (size/get on the copied map).
 * L6  the null-argument NPE (putMapEntries reads m.size() first).
 * L7  the null-key carry (HashMap allows a null key; the copy keeps it).
 * L8  the empty-source law: table stays lazy (threshold 0); the first
 *     put allocates the DEFAULT cap 16.
 * L9  the post-copy growth timing: threshold 12 after the copy of the
 *     11-entry source; the 12th entry does NOT resize (12 > 12 is
 *     false), the 13th does (the cap-32 split preserves chain order:
 *     key 20 moves to its own high bucket).
 */
public class HashMapCopyProbe {
    static void order(Map<Integer, Integer> m, String tag) {
        StringBuilder sb = new StringBuilder();
        for (Integer k : m.keySet()) sb.append(k).append(' ');
        System.out.println(tag + " size=" + m.size() + " order=" + sb.toString().trim());
    }

    public static void main(String[] args) throws Exception {
        // L1
        LinkedHashMap<Integer, Integer> lhm = new LinkedHashMap<>();
        lhm.put(16, 160);
        lhm.put(0, 100);
        lhm.put(32, 320);
        HashMap<Integer, Integer> c1 = new HashMap<>(lhm);
        order(c1, "L1-lhm-src");

        // L2
        LinkedHashMap<Integer, Integer> s11 = new LinkedHashMap<>();
        for (int i = 0; i < 10; i++) s11.put(i, i * i);
        s11.put(20, 400);
        order(new HashMap<>(s11), "L2-s11-cap16");

        LinkedHashMap<Integer, Integer> s12 = new LinkedHashMap<>();
        for (int i = 0; i < 12; i++) s12.put(i, i * i);
        s12.put(20, 400);
        order(new HashMap<>(s12), "L2-s12-cap32");

        // L3
        HashMap<Integer, Integer> c2 = new HashMap<>(c1);
        order(c2, "L3-hm-src");

        // L4
        TreeMap<Integer, Integer> tm = new TreeMap<>();
        for (int i = 9; i >= 0; i--) tm.put(i * 3, i);
        HashMap<Integer, Integer> c3 = new HashMap<>(tm);
        order(c3, "L4-tree-src");
        LinkedHashMap<Integer, Integer> l4b = new LinkedHashMap<>(tm);
        order(l4b, "L4b-tree-src-lhm");

        // L5
        System.out.println("L5 get16=" + c1.get(16) + " get0=" + c1.get(0)
                + " get32=" + c1.get(32) + " size=" + c1.size());

        // L6
        try {
            Map<Integer, Integer> nulled = null;
            new HashMap<>(nulled);
            System.out.println("L6 no-throw");
        } catch (NullPointerException e) {
            System.out.println("L6 NPE");
        }

        // L7
        LinkedHashMap<Integer, Integer> nk = new LinkedHashMap<>();
        nk.put(null, 7);
        nk.put(1, 70);
        HashMap<Integer, Integer> c4 = new HashMap<>(nk);
        System.out.println("L7 nullKey=" + c4.get(null) + " contains=" + c4.containsKey(null)
                + " k1=" + c4.get(1));
        order(c4, "L7-order");

        // L8
        HashMap<Integer, Integer> empty = new HashMap<>();
        HashMap<Integer, Integer> c5 = new HashMap<>(empty);
        c5.put(1, 11);
        c5.put(17, 17);
        order(c5, "L8-empty-src");

        // L9
        HashMap<Integer, Integer> c6 = new HashMap<>(s11);
        c6.put(100, 100);
        order(c6, "L9-plus1");
        c6.put(101, 101);
        order(c6, "L9-plus2");

        System.out.println("done");
    }
}
