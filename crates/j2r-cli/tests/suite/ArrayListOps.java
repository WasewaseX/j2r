import java.util.ArrayList;
import java.util.Iterator;

public class ArrayListOps {
    public static void main(String[] args) {
        ArrayList<String> list = new ArrayList<>();
        list.add("a");
        list.add("b");
        list.add("c");
        System.out.println(list);
        System.out.println(list.size());
        System.out.println(list.isEmpty());
        System.out.println(list.contains("b"));
        System.out.println(list.indexOf("c"));
        System.out.println(list.indexOf("zz"));
        System.out.println(list.get(1));
        System.out.println(list.set(1, "x"));
        System.out.println(list);
        list.add(0, "head");
        System.out.println(list);
        System.out.println(list.remove(0));
        System.out.println(list.remove("x"));
        System.out.println(list.remove("nope"));
        System.out.println(list);

        // iterator + CME on structural modification
        ArrayList<Integer> nums = new ArrayList<>();
        for (int i = 0; i < 5; i++) nums.add(i * 10);
        Iterator<Integer> it = nums.iterator();
        System.out.println(it.next());
        System.out.println(it.hasNext());
        nums.add(50);
        try {
            it.next();
            System.out.println("no-cme");
        } catch (java.util.ConcurrentModificationException e) {
            System.out.println("cme:" + e.getMessage());
        }

        // fresh iterator over modified list, exhaustion -> NoSuchElementException
        Iterator<Integer> it2 = nums.iterator();
        while (it2.hasNext()) System.out.print(it2.next() + " ");
        System.out.println();
        try {
            it2.next();
        } catch (java.util.NoSuchElementException e) {
            System.out.println("nsee:" + e.getMessage());
        }

        // Iterator.remove is a loud unsupported runtime error here
        Iterator<Integer> it3 = nums.iterator();
        it3.next();
        try {
            it3.remove();
        } catch (UnsupportedOperationException e) {
            System.out.println("rm:" + e.getMessage());
        }

        // bounds messages (three-channel pinned)
        try { nums.get(-1); } catch (IndexOutOfBoundsException e) { System.out.println("b1:" + e.getMessage()); }
        try { nums.get(nums.size()); } catch (IndexOutOfBoundsException e) { System.out.println("b2:" + e.getMessage()); }
        try { nums.add(99, 1); } catch (IndexOutOfBoundsException e) { System.out.println("b3:" + e.getMessage()); }
        try { nums.remove(99); } catch (IndexOutOfBoundsException e) { System.out.println("b4:" + e.getMessage()); }

        // clear + nulls
        ArrayList<String> nulls = new ArrayList<>();
        nulls.add(null);
        nulls.add("z");
        System.out.println(nulls);
        nulls.clear();
        System.out.println(nulls + " " + nulls.size() + " " + nulls.isEmpty());

        // boxed identity through the list (extends M1.6 BoxingIdentity)
        ArrayList<Integer> boxes = new ArrayList<>();
        boxes.add(127);
        boxes.add(128);
        Integer a = boxes.get(0);
        Integer b = 127;
        System.out.println("cacheIdentity:" + (a == b));
        System.out.println("bigIdentity:" + (boxes.get(1) == boxes.get(1)));

        // negative capacity ctor
        try { new ArrayList<String>(-5); } catch (IllegalArgumentException e) { System.out.println("cap:" + e.getMessage()); }
    }
}
