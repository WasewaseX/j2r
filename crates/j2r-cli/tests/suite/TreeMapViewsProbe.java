import java.util.TreeMap;
import java.util.SortedMap;
import java.util.NavigableMap;
import java.util.Map;
import java.util.Iterator;

public class TreeMapViewsProbe {
    public static void main(String[] a) {
        // base map for the slice cases: {1=a, 3=c, 5=e, 7=g, 9=i}
        TreeMap<Integer, String> t = new TreeMap<>();
        t.put(1, "a"); t.put(3, "c"); t.put(5, "e"); t.put(7, "g"); t.put(9, "i");

        // c01. absolute subMap(K,K): contents (from inclusive, to EXCLUSIVE)
        SortedMap<Integer, String> sub = t.subMap(3, 7);
        System.out.println("c01: " + sub);

        // c02. absolute headMap(K) / tailMap(K)
        System.out.println("c02: head=" + t.headMap(5) + " tail=" + t.tailMap(5));

        // c03. live view: put through the view at the boundary keys
        sub.put(4, "d4");
        System.out.println("c03: view=" + sub + " parent=" + t);
        sub.remove(4);
        System.out.println("c03b: view=" + sub + " parent=" + t);

        // c04. four-arg subMap inclusive variants
        System.out.println("c04: ii=" + t.subMap(3, true, 7, true)
            + " ee=" + t.subMap(3, false, 7, false)
            + " ie=" + t.subMap(3, true, 7, false)
            + " ei=" + t.subMap(3, false, 7, true));

        // c05. four-arg headMap/tailMap inclusive variants
        System.out.println("c05: headI=" + t.headMap(5, true)
            + " tailE=" + t.tailMap(5, false));

        // c06. view size/isEmpty are COMPUTED over the range, not mirrors
        System.out.println("c06: size=" + sub.size() + " empty=" + sub.isEmpty());
        SortedMap<Integer, String> none = t.subMap(10, 20);
        System.out.println("c06b: none=" + none + " size=" + none.size()
            + " empty=" + none.isEmpty());

        // c07. firstKey/lastKey on views + NSEE on an empty view
        System.out.println("c07: first=" + sub.firstKey() + " last=" + sub.lastKey());
        try { none.firstKey(); System.out.println("c07b: NO-THROW"); }
        catch (java.util.NoSuchElementException e) { System.out.println("c07b: NSEE"); }
        try { none.lastKey(); System.out.println("c07c: NO-THROW"); }
        catch (java.util.NoSuchElementException e) { System.out.println("c07c: NSEE"); }

        // c08. get: in-range hit/miss; OUT-OF-RANGE key -> null (silent)
        System.out.println("c08: get3=" + sub.get(3) + " get6=" + sub.get(6)
            + " get1=" + sub.get(1) + " get7=" + sub.get(7) + " get99=" + sub.get(99));

        // c09. containsKey: out-of-range -> false (silent)
        System.out.println("c09: ck3=" + sub.containsKey(3) + " ck1=" + sub.containsKey(1)
            + " ck7=" + sub.containsKey(7));

        // c10. getOrDefault: out-of-range -> the default
        System.out.println("c10: gd3=" + sub.getOrDefault(3, "D")
            + " gd1=" + sub.getOrDefault(1, "D"));

        // c11. remove: out-of-range -> null (silent), in-range removes
        System.out.println("c11: rm1=" + sub.remove(1) + " rm3=" + sub.remove(3)
            + " parent=" + t);
        t.put(3, "c"); // restore
        System.out.println("c11b: restored=" + sub);

        // c12. put out-of-range -> IAE "key out of range" (message pinned)
        try { sub.put(1, "x"); System.out.println("c12: NO-THROW"); }
        catch (IllegalArgumentException e) { System.out.println("c12: IAE " + e.getMessage()); }
        try { sub.put(7, "x"); System.out.println("c12b: NO-THROW"); }
        catch (IllegalArgumentException e) { System.out.println("c12b: IAE " + e.getMessage()); }
        try { sub.put(99, "x"); System.out.println("c12c: NO-THROW"); }
        catch (IllegalArgumentException e) { System.out.println("c12c: IAE " + e.getMessage()); }

        // c13. live both ways: parent put in-range visible via view,
        //      parent put out-of-range NOT visible; containsValue rides the slice
        t.put(6, "six");
        System.out.println("c13: view=" + sub + " cv=" + sub.containsValue("six")
            + " cvOut=" + sub.containsValue("a"));
        t.remove(6);
        System.out.println("c13b: view=" + sub);

        // c14. toString formats: empty view vs one-entry view
        System.out.println("c14: empty=" + t.subMap(10, 20) + " one=" + t.subMap(1, 2));

        // c15. slice-only clear: the slice empties, out-of-range keys survive
        SortedMap<Integer, String> slice = t.subMap(3, 9);
        slice.clear();
        System.out.println("c15: slice=" + slice + " parent=" + t
            + " parentSize=" + t.size());
        t.put(3, "c"); t.put(5, "e"); t.put(7, "g"); // restore {1,3,5,7,9}

        // c16. THE NARROWING MATRIX (chained builders on bounded views)
        SortedMap<Integer, String> s37 = t.subMap(3, 7);   // lo=3 inc, hi=7 EXC
        SortedMap<Integer, String> tail5 = t.tailMap(5);   // lo=5 inc, unbounded hi
        SortedMap<Integer, String> head7 = t.headMap(7);   // unbounded lo, hi=7 EXC
        // a. re-request the same bounds: toKey == the exclusive hi
        try { s37.subMap(3, 7); System.out.println("c16a: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16a: IAE " + e.getMessage()); }
        // b. strict interior
        System.out.println("c16b: " + s37.subMap(4, 6));
        // c. below the lo
        try { s37.subMap(2, 5); System.out.println("c16c: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16c: IAE " + e.getMessage()); }
        // d. the inclusive lo boundary as fromKey
        System.out.println("c16d: " + s37.subMap(3, 6));
        // e. headMap to the exclusive hi
        try { s37.headMap(7); System.out.println("c16e: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16e: IAE " + e.getMessage()); }
        // f. tailMap from the exclusive hi
        try { s37.tailMap(7); System.out.println("c16f: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16f: IAE " + e.getMessage()); }
        // g. headMap down to the inclusive lo
        System.out.println("c16g: " + s37.headMap(3));
        // h. tailMap from the inclusive lo
        System.out.println("c16h: " + s37.tailMap(3));
        // i. on an unbounded-hi view: headMap ACCEPTS toKey that becomes a new exclusive hi
        System.out.println("c16i: " + tail5.headMap(7));
        // j. on an exclusive-hi view: tailMap REJECTS fromKey == that hi
        try { head7.tailMap(7); System.out.println("c16j: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16j: IAE " + e.getMessage()); }
        // k. above the exclusive hi on an unbounded-hi parent: tailMap accepts
        System.out.println("c16k: " + tail5.tailMap(9));
        // l. crossed bounds on the ABSOLUTE map
        try { t.subMap(9, 3); System.out.println("c16l: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16l: IAE " + e.getMessage()); }
        // m. from == to on the absolute map: legal empty view
        System.out.println("c16m: " + t.subMap(5, 5));
        // n. chained twice
        System.out.println("c16n: " + t.subMap(3, 9).subMap(5, 7));
        // o. no-intersection above: chained narrowing throws, never an empty map
        try { tail5.headMap(7).tailMap(7); System.out.println("c16o: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16o: IAE " + e.getMessage()); }
        // p. no-intersection below
        try { s37.subMap(1, 2); System.out.println("c16p: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16p: IAE " + e.getMessage()); }
        // q. crossed bounds INSIDE a legal window (both in range)
        try { t.subMap(3, 9).subMap(8, 5); System.out.println("c16q: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16q: IAE " + e.getMessage()); }
        // r. absolute headMap/tailMap accept any single key (no range to violate)
        System.out.println("c16r: " + t.headMap(-99) + " " + t.tailMap(99));
        // s. subMap's fromKey == the exclusive hi
        try { s37.subMap(7, 7); System.out.println("c16s: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16s: IAE " + e.getMessage()); }
        // t. tailMap's fromKey above the hi
        SortedMap<Integer, String> s19 = t.subMap(1, 9);
        try { s19.tailMap(99); System.out.println("c16t: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16t: IAE " + e.getMessage()); }
        // u. headMap cross below the lo (bounded-lo parent)
        try { s37.headMap(2); System.out.println("c16u: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16u: IAE " + e.getMessage()); }
        // v. headMap cross below the toKey on an UNBOUNDED-lo parent (fromStart)
        System.out.println("c16v: " + head7.headMap(2));
        // w. chained subMap with toKey BELOW the lo (cross vs toKey-range order)
        try { t.subMap(1, 9).subMap(5, 0); System.out.println("c16w: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16w: IAE " + e.getMessage()); }
        // x. chained subMap with fromKey BELOW the lo AND crossed (order pin)
        try { s37.subMap(2, 1); System.out.println("c16x: OK"); }
        catch (IllegalArgumentException e) { System.out.println("c16x: IAE " + e.getMessage()); }
        // y. headMap == hi on a HI-INCLUSIVE parent (strict-> accepts)
        System.out.println("c16y: " + t.subMap(3, true, 7, true).headMap(7));
        // z. chained subMap on an unbounded-lo parent (fromKey below the lo-key)
        System.out.println("c16z: " + head7.subMap(2, 5));

        // c17. the four exact IAE messages, one per shape
        SortedMap<Integer, String> big = t.subMap(1, 9);
        try { big.put(0, "x"); } catch (IllegalArgumentException e) {
            System.out.println("c17a: " + e.getMessage()); }
        try { big.subMap(0, 5); } catch (IllegalArgumentException e) {
            System.out.println("c17b: " + e.getMessage()); }
        try { big.subMap(5, 99); } catch (IllegalArgumentException e) {
            System.out.println("c17c: " + e.getMessage()); }
        try { big.subMap(8, 2); } catch (IllegalArgumentException e) {
            System.out.println("c17d: " + e.getMessage()); }

        // c18. null keys on bounded views (natural ordering)
        try { sub.put(null, "x"); System.out.println("c18a: NO-THROW"); }
        catch (NullPointerException e) { System.out.println("c18a: NPE"); }
        try { System.out.println("c18b: " + sub.get(null)); }
        catch (NullPointerException e) { System.out.println("c18b: NPE"); }
        try { System.out.println("c18c: " + t.headMap(5).get(null)); }
        catch (NullPointerException e) { System.out.println("c18c: NPE"); }
        try { System.out.println("c18d: " + t.tailMap(5).get(null)); }
        catch (NullPointerException e) { System.out.println("c18d: NPE"); }

        // c19. non-Comparable keys on bounded views (raw-typed view so the
        //      compiler lets a bare Object reach the range check)
        TreeMap<Integer, String> ti = new TreeMap<>();
        ti.put(1, "a"); ti.put(5, "e"); ti.put(9, "i");
        @SuppressWarnings("unchecked")
        SortedMap<Object, Object> ob = (SortedMap<Object, Object>) (SortedMap) ti.subMap(1, 9);
        try { ob.put(new Object(), "x"); System.out.println("c19a: NO-THROW"); }
        catch (IllegalArgumentException e) { System.out.println("c19a: IAE"); }
        catch (ClassCastException e) { System.out.println("c19a: CCE"); }
        try { System.out.println("c19b: " + ob.get(new Object())); }
        catch (ClassCastException e) { System.out.println("c19b: CCE"); }

        // c20. comparator() rides through the view (null for natural ordering)
        System.out.println("c20: cmpNull=" + sub.comparator());
        TreeMap<Integer, String> desc = new TreeMap<Integer, String>((x, y) -> y - x);
        desc.put(1, "a"); desc.put(3, "c"); desc.put(5, "e"); desc.put(7, "g");
        NavigableMap<Integer, String> dsub = desc.subMap(7, true, 3, true);
        System.out.println("c20b: desc=" + dsub + " cmp=" + (dsub.comparator() != null));

        // c21. view keySet: order, toString, size
        java.util.Set<Integer> ks = sub.keySet();
        StringBuilder sb21 = new StringBuilder();
        for (Integer k : ks) sb21.append(k).append(" ");
        System.out.println("c21: " + sb21 + "| " + ks + " size=" + ks.size());

        // c22. view values: order, toString
        java.util.Collection<String> vs = sub.values();
        StringBuilder sb22 = new StringBuilder();
        for (String v : vs) sb22.append(v).append(" ");
        System.out.println("c22: " + sb22 + "| " + vs);

        // c23. view entrySet: getKey/getValue + toString
        StringBuilder sb23 = new StringBuilder();
        for (Map.Entry<Integer, String> e : sub.entrySet())
            sb23.append(e.getKey()).append("=").append(e.getValue()).append(";");
        System.out.println("c23: " + sb23 + "| " + sub.entrySet());

        // c24. bounded key iterator: mid-walk remove, order intact, no CME
        Iterator<Integer> it = sub.keySet().iterator();
        StringBuilder sb24 = new StringBuilder();
        while (it.hasNext()) {
            Integer k = it.next();
            if (k == 5) it.remove();
            else sb24.append(k).append(" ");
        }
        System.out.println("c24: " + sb24 + "| view=" + sub + " parent=" + t);
        t.put(5, "e"); // restore

        // c25. bounded iterator: remove without next -> ISE
        Iterator<Integer> it2 = sub.keySet().iterator();
        try { it2.remove(); System.out.println("c25: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("c25: ISE"); }

        // c26. bounded iterator: foreign in-range put through the PARENT -> CME
        Iterator<Integer> it3 = sub.keySet().iterator();
        it3.next();
        t.put(6, "six");
        try { it3.next(); System.out.println("c26: NO-THROW"); }
        catch (java.util.ConcurrentModificationException e) { System.out.println("c26: CME"); }
        t.remove(6);

        // c27. bounded iterator: foreign OUT-OF-RANGE put still CMEs
        //      (expectedModCount is the BACKING map's modCount)
        Iterator<Integer> it4 = sub.keySet().iterator();
        it4.next();
        t.put(99, "zz");
        try { it4.next(); System.out.println("c27: NO-THROW"); }
        catch (java.util.ConcurrentModificationException e) { System.out.println("c27: CME"); }
        t.remove(99);

        // c28. bounded iterator: double remove -> ISE; exhausted walk: hasNext false
        Iterator<Integer> it5 = sub.keySet().iterator();
        it5.next();
        it5.remove();
        try { it5.remove(); System.out.println("c28a: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("c28a: ISE"); }
        while (it5.hasNext()) it5.next();
        System.out.println("c28b: exhausted=" + !it5.hasNext());
        try { it5.next(); System.out.println("c28c: NO-THROW"); }
        catch (java.util.NoSuchElementException e) { System.out.println("c28c: NSEE"); }
        t.put(5, "e"); // restore {1,3,5,7,9}

        // c29. the empty-intersection view from inclusive bounds is puttable
        NavigableMap<Integer, String> single = t.subMap(5, true, 5, true);
        System.out.println("c29: " + single + " size=" + single.size()
            + " first=" + single.firstKey() + " last=" + single.lastKey());
        single.put(5, "FIVE");
        System.out.println("c29b: " + single + " parent5=" + t.get(5));
        single.remove(5);
        System.out.println("c29c: " + single + " parent5=" + t.get(5));
        t.put(5, "e"); // restore {1,3,5,7,9}

        // c30. headMap below every key / tailMap above every key: empty views
        System.out.println("c30: low=" + t.headMap(0) + " high=" + t.tailMap(100));

        // c31. exclusive-lo tailMap at an existing key excludes it
        System.out.println("c31: " + t.tailMap(5, false));

        // c32. view of a view: double narrowing contents + size + first/last
        SortedMap<Integer, String> vv = t.subMap(3, 9).subMap(5, 7);
        System.out.println("c32: " + vv + " size=" + vv.size()
            + " first=" + vv.firstKey() + " last=" + vv.lastKey());

        // c33. bounded ENTRY iterator remove + continued walk (ISE-first carries)
        Iterator<Map.Entry<Integer, String>> ei = t.subMap(1, 9).entrySet().iterator();
        StringBuilder sb33 = new StringBuilder();
        while (ei.hasNext()) {
            Map.Entry<Integer, String> e = ei.next();
            if (e.getKey() == 3) ei.remove();
            else sb33.append(e.getKey()).append(" ");
        }
        System.out.println("c33: " + sb33 + "| parent=" + t);
        t.put(3, "c"); // restore

        // c34. remove on an empty derived view: out-of-range key -> IAE
        SortedMap<Integer, String> e1 = t.subMap(10, 20);
        try { e1.remove(1); System.out.println("c34a: " + e1); }
        catch (IllegalArgumentException ex) { System.out.println("c34a: IAE"); }

        // c35. 50-key stress: range walk, slice clear, parent survives
        TreeMap<Integer, Integer> big2 = new TreeMap<>();
        for (int i = 0; i < 50; i++) big2.put(i, i * i);
        SortedMap<Integer, Integer> mid = big2.subMap(10, 20);
        int sum = 0;
        for (Integer k : mid.keySet()) sum += k;
        System.out.println("c35: sum=" + sum + " size=" + mid.size()
            + " parent=" + big2.size() + " first=" + mid.firstKey()
            + " last=" + mid.lastKey());
        mid.clear();
        System.out.println("c35b: cleared=" + mid.size() + " parent=" + big2.size()
            + " has10=" + big2.containsKey(10) + " has20=" + big2.containsKey(20)
            + " has9=" + big2.containsKey(9) + " has21=" + big2.containsKey(21));
        System.out.println("done");
    }
}
