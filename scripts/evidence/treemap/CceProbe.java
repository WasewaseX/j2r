import java.util.TreeMap;
public class CceProbe {
    public static void main(String[] a) {
        TreeMap<Object, String> t = new TreeMap<>();
        try { t.put(new Object(), "x"); System.out.println("NO-THROW"); }
        catch (ClassCastException e) { System.out.println("CCE"); }
        catch (Throwable e) { System.out.println("OTHER: " + e.getClass().getName()); }
    }
}
