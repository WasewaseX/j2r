import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;

public class ForEachColl {
    public static void main(String[] args) {
        // enhanced-for over ArrayList (Iterable interface dispatch)
        List<String> list = new ArrayList<>();
        list.add("a");
        list.add("b");
        list.add("c");
        StringBuilder joined = new StringBuilder();
        for (String s : list) joined.append(s).append('-');
        System.out.println(joined);

        // enhanced-for over a HashMap keySet and values view
        Map<Integer, String> m = new HashMap<>();
        for (int i = 0; i < 7; i++) m.put(i * 3, "w" + i);
        int keySum = 0;
        for (Integer k : m.keySet()) keySum += k;
        System.out.println(keySum);
        StringBuilder vals = new StringBuilder();
        for (String v : m.values()) vals.append(v).append('.');
        System.out.println(vals);

        // nested loops over the same collection (independent iterators)
        int pairs = 0;
        for (String x : list) for (String y : list) if (x.equals(y)) pairs++;
        System.out.println(pairs);

        // enhanced-for over HashSet
        HashSet<Integer> set = new HashSet<>();
        for (int i = 0; i < 10; i++) set.add(i * 11);
        int setSum = 0;
        for (Integer n : set) setSum += n;
        System.out.println(setSum);

        // unboxing through enhanced-for (generic-syntax source, erased core)
        List<Integer> boxes = new ArrayList<>();
        boxes.add(10);
        boxes.add(20);
        int total = 0;
        for (int v : boxes) total += v;
        System.out.println(total);

        // forEach-free summation via explicit iterator on the List interface
        java.util.Iterator<String> it = list.iterator();
        int len = 0;
        while (it.hasNext()) { len += it.next().length(); }
        System.out.println(len);
    }
}
