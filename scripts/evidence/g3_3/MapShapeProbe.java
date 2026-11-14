import java.util.HashMap;
import java.util.Map;

public class MapShapeProbe {
    public static void main(String[] args) {
        Map<String, Double> m = new HashMap<String, Double>();
        // the put core
        m.put("a", 0.5);
        // the get core (consumed by dadd)
        double x = m.get("a") + 1.0;
        // the put core with an arithmetic value
        m.put("a", x * 2.0);
        // the checksum-shaped get
        double chk = 0.0;
        for (int i = 0; i < 2; i++) {
            chk += m.get("a");
        }
        System.out.println(x + " " + chk);
    }
}
