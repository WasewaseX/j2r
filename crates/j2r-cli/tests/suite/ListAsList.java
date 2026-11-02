import java.util.ArrayList;
import java.util.List;

public class ListAsList {
    public static void main(String[] args) {
        // every call resolves through the java/util/List interface entries
        List<String> l = new ArrayList<>();
        l.add("one");
        l.add("two");
        l.add("three");
        System.out.println(l.size());
        System.out.println(l.isEmpty());
        System.out.println(l.get(1));
        System.out.println(l.set(0, "uno"));
        System.out.println(l.indexOf("three"));
        System.out.println(l.contains("two"));
        System.out.println(l.remove(2));
        System.out.println(l.remove("nope"));
        System.out.println(l);

        List<Integer> nums = new ArrayList<>();
        nums.add(5);
        nums.add(6);
        // interface-typed set + get with unboxing
        int x = nums.get(0);
        System.out.println(x + nums.get(1));

        // clear through the interface
        List<String> c = new ArrayList<>();
        c.add("x");
        c.clear();
        System.out.println(c + " " + c.size());

        // bounds through the interface (same pinned messages)
        try { l.get(-2); } catch (IndexOutOfBoundsException e) { System.out.println("g:" + e.getMessage()); }
        try { l.add(9, "y"); } catch (IndexOutOfBoundsException e) { System.out.println("a:" + e.getMessage()); }

        // ArrayList-typed variable still works after interface use
        ArrayList<String> raw = new ArrayList<>(0);
        raw.add("grown");
        System.out.println(raw);
        System.out.println(raw.size() + " " + raw.isEmpty());
    }
}
