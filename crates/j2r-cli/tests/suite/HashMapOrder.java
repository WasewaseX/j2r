import java.util.HashMap;

public class HashMapOrder {
    static class Key {
        final int v;
        Key(int v) { this.v = v; }
        public int hashCode() { return v * 31 + 7; }
        public boolean equals(Object o) { return o instanceof Key && ((Key) o).v == v; }
        public String toString() { return "K" + v; }
    }

    public static void main(String[] args) {
        // Integer keys: hash = value (spec-pinned) -> byte-identical order
        HashMap<Integer, String> m = new HashMap<>();
        for (int i = 0; i < 20; i++) m.put(i, "v" + i);
        System.out.println(m);
        System.out.println(m.size());

        // resize happened at the 13th put; order must match Temurin's table
        m.put(100, "hundred");
        m.put(101, "hundred-one");
        System.out.println(m.get(100));
        System.out.println(m.get(20));

        // replace returns the old value and is not a structural change
        System.out.println(m.put(5, "five"));
        System.out.println(m.get(5));

        // String keys (spec hash) with collisions
        HashMap<String, Integer> sm = new HashMap<>();
        String[] keys = {"Aa", "BB", "AaAa", "AaBB", "BBAa", "BBBB", "one", "two", "three", "four", "five", "six", "seven"};
        for (String k : keys) sm.put(k, k.length());
        System.out.println(sm);

        // user-class keys: dispatched hashCode/equals
        HashMap<Key, Integer> um = new HashMap<>();
        for (int i = 0; i < 14; i++) um.put(new Key(i), i);
        System.out.println(um.get(new Key(7)));
        System.out.println(um.containsKey(new Key(13)));
        System.out.println(um.containsKey(new Key(99)));
        System.out.println(um);

        // getOrDefault / containsKey / containsValue / remove
        System.out.println(m.getOrDefault(999, "dflt"));
        System.out.println(m.getOrDefault(5, "dflt"));
        System.out.println(m.containsKey(6));
        System.out.println(m.containsValue("v6"));
        System.out.println(m.containsValue("nope"));
        System.out.println(m.remove(19));
        System.out.println(m.remove(19));
        System.out.println(m.size());
        System.out.println(m.isEmpty());
        System.out.println(m.keySet());
        System.out.println(m.values());

        // null key and value
        HashMap<String, String> nm = new HashMap<>();
        nm.put(null, "nullkey");
        nm.put("v", null);
        System.out.println(nm);
        System.out.println(nm.get(null));

        // negative initial capacity
        try { new HashMap<String, String>(-3); } catch (IllegalArgumentException e) { System.out.println("cap:" + e.getMessage()); }

        // CME through keySet iterator
        HashMap<Integer, Integer> cm = new HashMap<>();
        for (int i = 0; i < 3; i++) cm.put(i, i);
        java.util.Iterator<Integer> it = cm.keySet().iterator();
        it.next();
        cm.put(50, 50);
        try {
            it.next();
            System.out.println("no-cme");
        } catch (java.util.ConcurrentModificationException e) {
            System.out.println("cme:" + e.getMessage());
        }
    }
}
