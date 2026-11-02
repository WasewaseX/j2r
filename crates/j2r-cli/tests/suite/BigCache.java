import java.lang.reflect.Field;
import java.util.ArrayList;

/**
 * BigCache — real-program-scale differential fixture: a from-scratch
 * open-addressing hash table, an LRU cache over a hand-built doubly-linked
 * list, a deterministic workload with invariant assertions, and a
 * reflection pass over the cache classes (M3 Wave 5 surface).
 *
 * No java.util collections on the hot path — the point is that the
 * *program* builds its own data structures, the way real libraries do.
 */
public class BigCache {

    // ------------------------------------------------------------------
    // Open-addressing int->long table (linear probing, tombstones)
    // ------------------------------------------------------------------

    static final class IntTable {
        private int[] keys;
        private long[] vals;
        private boolean[] used;
        private boolean[] tombstone;
        private int size;
        private int usedCount;

        IntTable(int cap) {
            if (cap < 4) {
                cap = 4;
            }
            keys = new int[cap];
            vals = new long[cap];
            used = new boolean[cap];
            tombstone = new boolean[cap];
        }

        private int probe(int key) {
            int mask = keys.length - 1;
            int i = key * 0x9E3779B9 >>> 3 & mask;
            while (used[i] && keys[i] != key) {
                i = i + 1 & mask;
            }
            return i;
        }

        void put(int key, long val) {
            if (usedCount * 4 >= keys.length * 3) {
                grow();
            }
            int i = probe(key);
            if (!used[i]) {
                if (tombstone[i]) {
                    // Reuse the tombstone slot; it held no live entry.
                } else {
                    usedCount++;
                }
                used[i] = true;
                tombstone[i] = false;
                size++;
            }
            keys[i] = key;
            vals[i] = val;
        }

        long get(int key) {
            int i = probe(key);
            return used[i] ? vals[i] : -1L;
        }

        boolean contains(int key) {
            return get(key) >= 0;
        }

        void remove(int key) {
            int i = probe(key);
            if (used[i]) {
                used[i] = false;
                tombstone[i] = true;
                size--;
            }
        }

        int size() {
            return size;
        }

        private void grow() {
            int[] oldKeys = keys;
            long[] oldVals = vals;
            boolean[] oldUsed = used;
            keys = new int[keys.length * 2];
            vals = new long[vals.length * 2];
            used = new boolean[used.length * 2];
            tombstone = new boolean[tombstone.length * 2];
            size = 0;
            usedCount = 0;
            for (int i = 0; i < oldKeys.length; i++) {
                if (oldUsed[i]) {
                    put(oldKeys[i], oldVals[i]);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // LRU over a hand-built doubly-linked list
    // ------------------------------------------------------------------

    static final class Node {
        final int key;
        String val;
        Node prev;
        Node next;

        Node(int key, String val) {
            this.key = key;
            this.val = val;
        }
    }

    static final class Lru {
        private final IntTable indexOf;
        private final ArrayList<Node> storage = new ArrayList<Node>();
        private Node head;
        private Node tail;
        private final int cap;
        int hits;
        int misses;
        int evictions;

        Lru(int cap) {
            this.cap = cap;
            this.indexOf = new IntTable(16);
        }

        private void unlink(Node n) {
            if (n.prev != null) {
                n.prev.next = n.next;
            } else {
                head = n.next;
            }
            if (n.next != null) {
                n.next.prev = n.prev;
            } else {
                tail = n.prev;
            }
            n.prev = null;
            n.next = null;
        }

        private void pushFront(Node n) {
            n.next = head;
            n.prev = null;
            if (head != null) {
                head.prev = n;
            }
            head = n;
            if (tail == null) {
                tail = n;
            }
        }

        String get(int key) {
            long idx = indexOf.get(key);
            if (idx < 0) {
                misses++;
                return null;
            }
            Node n = storage.get((int) idx);
            unlink(n);
            pushFront(n);
            hits++;
            return n.val;
        }

        void put(int key, String val) {
            long idx = indexOf.get(key);
            if (idx >= 0) {
                Node n = storage.get((int) idx);
                n.val = val;
                unlink(n);
                pushFront(n);
                return;
            }
            Node n = new Node(key, val);
            storage.add(n);
            indexOf.put(key, storage.size() - 1);
            pushFront(n);
            if (storage.size() > cap) {
                // Evict from the tail; keep the storage array dense by
                // moving the last node into the victim's slot.
                Node victim = tail;
                int vslot = (int) indexOf.get(victim.key);
                unlink(victim);
                indexOf.remove(victim.key);
                int last = storage.size() - 1;
                if (storage.get(last) != victim) {
                    Node mover = storage.get(last);
                    storage.set(vslot, mover);
                    indexOf.put(mover.key, vslot);
                }
                storage.remove(last);
                evictions++;
            }
        }

        int size() {
            return storage.size();
        }

        /** MRU-to-LRU order dump (deterministic). */
        String order() {
            StringBuilder sb = new StringBuilder();
            Node n = head;
            while (n != null) {
                if (sb.length() > 0) {
                    sb.append('>');
                }
                sb.append(n.key).append(':').append(n.val);
                n = n.next;
            }
            return sb.toString();
        }
    }

    // ------------------------------------------------------------------
    // Deterministic workload
    // ------------------------------------------------------------------

    static Lcg lcg = new Lcg(0xC0FFEE);

    static final class Lcg {
        long state;

        Lcg(long seed) {
            this.state = seed;
        }

        int next() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> 33);
        }
    }

    public static void main(String[] args) {
        // --- IntTable direct checks ---
        IntTable t = new IntTable(8);
        for (int i = 0; i < 100; i++) {
            t.put(i * 7 + 1, (long) i * 1000);
        }
        boolean tableOk = true;
        for (int i = 0; i < 100; i++) {
            if (t.get(i * 7 + 1) != (long) i * 1000) {
                tableOk = false;
            }
        }
        System.out.println("table 100 hits: " + tableOk + " size=" + t.size());
        t.remove(7 * 3 + 1);
        t.remove(7 * 8 + 1);
        System.out.println("after remove size=" + t.size()
            + " gone1=" + t.contains(7 * 3 + 1)
            + " gone2=" + t.contains(7 * 8 + 1)
            + " kept=" + t.contains(7 * 9 + 1));
        // Re-insert after removal (tombstone reuse).
        t.put(7 * 3 + 1, 9999);
        System.out.println("reinsert=" + t.get(7 * 3 + 1) + " size=" + t.size());
        // Negative keys and int extremes.
        t.put(-42, 42);
        t.put(Integer.MIN_VALUE, -1);
        t.put(Integer.MAX_VALUE, 1);
        System.out.println("neg=" + t.get(-42)
            + " min=" + t.get(Integer.MIN_VALUE)
            + " max=" + t.get(Integer.MAX_VALUE));

        // --- LRU workload: 5-capacity cache, 200 ops ---
        Lru lru = new Lru(5);
        for (int op = 0; op < 200; op++) {
            int key = lcg.next() % 8;
            if (key < 0) {
                key = -key;
            }
            if (op % 3 == 0) {
                lru.put(key, "v" + op);
            } else {
                String got = lru.get(key);
                if (got != null && !got.startsWith("v")) {
                    throw new IllegalStateException("corrupt value for " + key);
                }
            }
            // Invariant: list length matches table size.
            if (lru.size() != lru.storage.size()) {
                throw new IllegalStateException("size drift at op " + op);
            }
            if (lru.size() > 5) {
                throw new IllegalStateException("over capacity at op " + op);
            }
        }
        System.out.println("lru hits=" + lru.hits + " misses=" + lru.misses
            + " evictions=" + lru.evictions + " size=" + lru.size());
        System.out.println("final order: " + lru.order());

        // Deterministic known-pattern check: fill, touch, evict.
        Lru known = new Lru(3);
        known.put(1, "a");
        known.put(2, "b");
        known.put(3, "c");
        System.out.println("known: " + known.order());
        known.get(1);          // 1 becomes MRU
        known.put(4, "d");     // evicts 2 (LRU)
        System.out.println("after touch+evict: " + known.order()
            + " has2=" + (known.get(2) != null));
        System.out.println("has1=" + (known.get(1) != null)
            + " has3=" + (known.get(3) != null)
            + " has4=" + (known.get(4) != null));

        // --- Reflection pass over the cache classes (M3 Wave 5) ---
        // Class literals are the supported path (Class.forName is a
        // loud-unsupported compile error, not a runtime exception).
        Class<?> lc = Lru.class;
        Class<?> nc = Node.class;
        System.out.println("lru name=" + lc.getName() + " simple=" + lc.getSimpleName());
        System.out.println("node name=" + nc.getName() + " simple=" + nc.getSimpleName());
        try {
            Field capField = lc.getDeclaredField("cap");
            Field keyField = nc.getDeclaredField("key");
            System.out.println("cap type=" + capField.getType().getName());
            System.out.println("key type=" + keyField.getType().getName());
            System.out.println("lru isInstance=" + lc.isInstance(lru)
                + " node isInstance=" + nc.isInstance(lru));
            lc.getDeclaredField("nope");
            System.out.println("unreachable");
        } catch (NoSuchFieldException e) {
            System.out.println("nsf: " + e.getMessage());
        }

        System.out.println("BigCache done");
    }
}
