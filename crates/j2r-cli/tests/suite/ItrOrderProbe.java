import java.util.ConcurrentModificationException;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;

public class ItrOrderProbe {
    public static void main(String[] a) {
        // Case 1: remove() with no prior next() AND a foreign modification
        // (both ISE and CME conditions hold) — which exception wins?
        HashMap<String, Integer> m1 = new HashMap<>();
        m1.put("x", 1);
        Iterator<String> i1 = m1.keySet().iterator();
        m1.remove("x"); // foreign mod: expectedModCount stale
        try { i1.remove(); System.out.println("case1: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("case1: ISE"); }
        catch (ConcurrentModificationException e) { System.out.println("case1: CME"); }

        // Case 2: remove() with no prior next(), no foreign mod — ISE
        HashMap<String, Integer> m2 = new HashMap<>();
        m2.put("x", 1);
        Iterator<String> i2 = m2.keySet().iterator();
        try { i2.remove(); System.out.println("case2: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("case2: ISE"); }
        catch (ConcurrentModificationException e) { System.out.println("case2: CME"); }

        // Case 3: double remove() — ISE (current cleared after first)
        HashMap<String, Integer> m3 = new HashMap<>();
        m3.put("x", 1);
        Iterator<String> i3 = m3.keySet().iterator();
        i3.next(); i3.remove();
        try { i3.remove(); System.out.println("case3: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("case3: ISE"); }
        catch (ConcurrentModificationException e) { System.out.println("case3: CME"); }

        // Case 4: foreign mod, then iterator.next() — CME; then remove() (no re-next): CME or ISE?
        HashMap<String, Integer> m4 = new HashMap<>();
        m4.put("x", 1); m4.put("y", 2);
        Iterator<String> i4 = m4.keySet().iterator();
        i4.next(); // current set
        m4.remove("y"); // foreign mod
        try { i4.next(); System.out.println("case4next: NO-THROW"); }
        catch (java.util.NoSuchElementException e) { System.out.println("case4next: NSEE"); }
        catch (ConcurrentModificationException e) { System.out.println("case4next: CME"); }
        try { i4.remove(); System.out.println("case4rm: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("case4rm: ISE"); }
        catch (ConcurrentModificationException e) { System.out.println("case4rm: CME"); }

        // Case 5: LinkedHashMap — same combined case
        LinkedHashMap<String, Integer> l1 = new LinkedHashMap<>();
        l1.put("x", 1);
        Iterator<String> li = l1.keySet().iterator();
        l1.remove("x");
        try { li.remove(); System.out.println("case5: NO-THROW"); }
        catch (IllegalStateException e) { System.out.println("case5: ISE"); }
        catch (ConcurrentModificationException e) { System.out.println("case5: CME"); }

        // Case 6: values() + entrySet() iterator remove basic laws
        HashMap<String, Integer> m6 = new HashMap<>();
        m6.put("a", 1); m6.put("b", 2);
        Iterator<Integer> vi = m6.values().iterator();
        vi.next(); vi.remove();
        System.out.println("case6: size=" + m6.size() + " hasB=" + m6.containsKey("b"));
        Iterator<Map.Entry<String, Integer>> ei = m6.entrySet().iterator();
        Map.Entry<String, Integer> e = ei.next();
        ei.remove();
        System.out.println("case7: size=" + m6.size() + " removed=" + e.getKey());

        // Case 8: HashSet-backed remove (HashSet$KeyIterator path)
        java.util.HashSet<String> hs = new java.util.HashSet<>();
        hs.add("a"); hs.add("b");
        Iterator<String> hi = hs.iterator();
        hi.next(); hi.remove();
        System.out.println("case8: size=" + hs.size());

        // Case 9: continued iteration after remove — no CME, remaining keys seen
        HashMap<String, Integer> m9 = new HashMap<>();
        m9.put("k1", 1); m9.put("k2", 2); m9.put("k3", 3);
        Iterator<String> i9 = m9.keySet().iterator();
        StringBuilder sb = new StringBuilder();
        while (i9.hasNext()) {
            String k = i9.next();
            if (k.equals("k2")) i9.remove();
            sb.append(k).append(";");
        }
        System.out.println("case9: walked=" + sb + " size=" + m9.size() + " has2=" + m9.containsKey("k2"));
    }
}
