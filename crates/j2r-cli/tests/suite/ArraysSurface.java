import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;

/**
 * ArraysSurface — differential fixture for the real-program JDK surface:
 * java.util.Arrays (toString/fill/copyOf/copyOfRange/equals/sort),
 * java.util.Collections (sort/reverse/swap), and ArrayList.subList with
 * live view semantics (modCount discipline, JDK-exact exception texts).
 *
 * Object sorts must be STABLE (TimSort's guarantee): equal keys keep their
 * original relative order. The transcript pins that against Temurin.
 */
public class ArraysSurface {

    public static void main(String[] args) {
        // --- toString family ---
        int[] ints = {3, -1, 0, 42};
        long[] longs = {5L, -9000000000000000000L, 9000000000000000000L};
        double[] ds = {1.5, -0.0, 0.0, Double.NaN, 2.0};
        boolean[] bs = {true, false};
        char[] cs = {'a', 'B', '\n'};
        String[] objs = {"x", null, "y z"};
        System.out.println(Arrays.toString(ints));
        System.out.println(Arrays.toString(longs));
        System.out.println(Arrays.toString(ds));
        System.out.println(Arrays.toString(bs));
        System.out.println(Arrays.toString(cs));
        System.out.println(Arrays.toString(objs));
        System.out.println(Arrays.toString(new int[0]));

        // --- fill ---
        int[] f = new int[4];
        Arrays.fill(f, 7);
        System.out.println(Arrays.toString(f));
        Object[] fo = new String[3];
        Arrays.fill(fo, "hi");
        System.out.println(Arrays.toString(fo));
        long[] fl = new long[2];
        Arrays.fill(fl, -1L);
        System.out.println(Arrays.toString(fl));

        // --- copyOf: grow (null/zero padding), shrink ---
        int[] src = {1, 2, 3};
        System.out.println(Arrays.toString(Arrays.copyOf(src, 5)));
        System.out.println(Arrays.toString(Arrays.copyOf(src, 2)));
        String[] so = {"a", "b"};
        String[] grown = Arrays.copyOf(so, 4);
        System.out.println(Arrays.toString(grown) + " len=" + grown.length);
        // Generic 3-arg form javac emits for T[].
        String[] gen = Arrays.copyOf(so, 3, String[].class);
        System.out.println(Arrays.toString(gen));

        // --- copyOfRange: normal, clamp-past-end, crossed bounds ---
        System.out.println(Arrays.toString(Arrays.copyOfRange(src, 1, 3)));
        System.out.println(Arrays.toString(Arrays.copyOfRange(src, 1, 6)));
        try {
            Arrays.copyOfRange(src, 3, 2);
            System.out.println("unreachable");
        } catch (IllegalArgumentException e) {
            System.out.println("iae: " + e.getMessage());
        }

        // --- equals ---
        System.out.println(Arrays.equals(src, Arrays.copyOf(src, 3)));
        System.out.println(Arrays.equals(src, new int[]{1, 2}));
        System.out.println(Arrays.equals(new String[]{null, "x"}, new String[]{null, "x"}));
        System.out.println(Arrays.equals(new String[]{"x"}, new String[]{"X"}));

        // --- sort primitives (incl. double total order: -0.0 < 0.0, NaN last) ---
        int[] si = {5, -3, 5, 0, -3, 2};
        Arrays.sort(si);
        System.out.println(Arrays.toString(si));
        long[] sl = {2L, -9223372036854775808L, 0L, 9223372036854775807L};
        Arrays.sort(sl);
        System.out.println(Arrays.toString(sl));
        double[] sd = {0.0, -0.0, Double.NaN, -1.5, Double.NaN, 3.25};
        Arrays.sort(sd);
        System.out.println(Arrays.toString(sd));

        // --- natural-ordering object sort (String compareTo) ---
        String[] names = {"pear", "Apple", "apple", "Pear", "banana", ""};
        Arrays.sort(names);
        System.out.println(Arrays.toString(names));

        // --- comparator sort with a lambda ---
        Integer[] nums = {5, 3, 9, 3, 1};
        Arrays.sort(nums, (a, b) -> b - a);
        System.out.println(Arrays.toString(nums));

        // --- STABILITY: equal keys keep insertion order (TimSort pin) ---
        String[] recs = {"b2", "a1", "b1", "a3", "b3", "a2"};
        Arrays.sort(recs, (x, y) -> Character.compare(x.charAt(0), y.charAt(0)));
        System.out.println(Arrays.toString(recs));

        // --- NULL comparator: the JDK source's first branch falls back to
        // natural ordering — must NOT throw NPE ---
        String[] nats = {"bb", "a", "cc", "b", "aa"};
        Arrays.sort(nats, (Comparator<String>) null);
        System.out.println(Arrays.toString(nats));
        Integer[] natsN = {7, -1, 3};
        Arrays.sort(natsN, (Comparator<Integer>) null);
        System.out.println(Arrays.toString(natsN));

        // --- Collections.sort on a List, both forms ---
        List<String> words = new ArrayList<String>();
        words.add("delta");
        words.add("alpha");
        words.add("charlie");
        words.add("bravo");
        Collections.sort(words);
        System.out.println(words);
        Collections.sort(words, (a, b) -> b.compareTo(a));
        System.out.println(words);

        // Natural sort over boxed Integers (compareTo bridge dispatch).
        List<Integer> iqs = new ArrayList<Integer>();
        iqs.add(42);
        iqs.add(-7);
        iqs.add(0);
        iqs.add(2000000000);
        Collections.sort(iqs);
        System.out.println(iqs);

        // Doubles: total order through the bridge (NaN top, -0 < +0).
        List<Double> dqs = new ArrayList<Double>();
        dqs.add(0.0);
        dqs.add(-0.0);
        dqs.add(Double.NaN);
        dqs.add(1.5);
        Collections.sort(dqs);
        System.out.println(dqs);

        // --- Collections.reverse / swap ---
        Collections.reverse(words);
        System.out.println(words);
        Collections.swap(words, 0, words.size() - 1);
        System.out.println(words);

        // --- subList: live view semantics ---
        List<Integer> base = new ArrayList<Integer>();
        for (int i = 0; i < 10; i++) {
            base.add(i * 10);
        }
        List<Integer> view = base.subList(2, 6);
        System.out.println("view=" + view + " size=" + view.size()
            + " empty=" + view.isEmpty());
        System.out.println("view.get(0)=" + view.get(0) + " get(3)=" + view.get(3));
        view.set(1, 999);
        System.out.println("after set: parent=" + base.get(3) + " view=" + view);
        view.add(2, 555);
        System.out.println("after add: view=" + view + " baseSize=" + base.size());
        Object removed = view.remove(0);
        System.out.println("removed=" + removed + " view=" + view);

        // Chained: sort through the view (Collections.sort on a view).
        Collections.sort(view);
        System.out.println("sorted view=" + view + " base=" + base);

        // Mutating the parent directly invalidates the view: CME.
        try {
            base.add(12345);
            view.get(0);
            System.out.println("unreachable");
        } catch (RuntimeException e) {
            System.out.println(e.getClass().getName()
                + ": " + e.getMessage());
        }
        // A fresh view after the mutation is fine.
        List<Integer> fresh = base.subList(0, 3);
        System.out.println("fresh=" + fresh);

        // Bounds messages, JDK-exact.
        try {
            base.subList(-1, 3);
        } catch (IndexOutOfBoundsException e) {
            System.out.println("ioobe: " + e.getMessage());
        }
        try {
            base.subList(0, 99);
        } catch (IndexOutOfBoundsException e) {
            System.out.println("ioobe: " + e.getMessage());
        }
        try {
            base.subList(5, 2);
        } catch (IllegalArgumentException e) {
            System.out.println("iae: " + e.getMessage());
        }
        try {
            view = base.subList(0, 3);
            view.get(3);
        } catch (IndexOutOfBoundsException e) {
            System.out.println("ioobe: " + e.getMessage());
        }

        System.out.println("ArraysSurface done");
    }
}
