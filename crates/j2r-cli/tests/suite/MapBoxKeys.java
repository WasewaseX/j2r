import java.util.HashMap;
import java.util.Map;

public class MapBoxKeys {
    public static void main(String[] args) {
        // Long keys (hash = value) with resize past the 13th put
        Map<Long, String> m = new HashMap<>();
        for (long k = 0; k < 20; k++) m.put(k, "L" + k);
        System.out.println(m.size());
        System.out.println(m.get(0L));
        System.out.println(m.get(19L));
        System.out.println(m.get(100L));
        System.out.println(m);

        // Double keys: hash is the Double.hashCode bits — pinned by oracle
        Map<Double, String> dm = new HashMap<>();
        dm.put(1.5, "one");
        dm.put(2.25, "two");
        dm.put(-0.75, "neg");
        dm.put(0.0, "zero");
        dm.put(-0.0, "negzero");
        System.out.println(dm.get(1.5));
        System.out.println(dm.get(-0.75));
        System.out.println(dm.size());
        System.out.println(dm.get(0.0));
        System.out.println(dm.get(-0.0));

        // Character keys
        Map<Character, Integer> cm = new HashMap<>();
        for (char c = 'a'; c < 'a' + 15; c++) cm.put(c, (int) c);
        System.out.println(cm.size());
        System.out.println(cm.get('c'));
        System.out.println(cm);

        // Long/Character boxed identity at JLS cache boundaries
        Long a = 127L;
        Long b = 127L;
        System.out.println("longCache:" + (a == b));
        Long c = 128L;
        Long d = 128L;
        System.out.println("longNoCache:" + (c == d));
        Character e1 = '\u0000';
        Character e2 = '\u0000';
        System.out.println("charCache:" + (e1 == e2));
    }
}
