import java.util.*;
public class NpeShapes4 {
    static List<String> getList() { return null; }
    static void runA(String[] arr) {
        try { int n = arr.length; System.out.println("B1 unreachable " + n); }
        catch (NullPointerException e) { System.out.println("B1: msg=[" + e.getMessage() + "]"); }
    }
    static void runB(String[][] arr) {
        try { int n = arr.length; System.out.println("B2 unreachable " + n); }
        catch (NullPointerException e) { System.out.println("B2: msg=[" + e.getMessage() + "]"); }
    }
    public static void main(String[] args) {
        runA(null); runB(null);
        // B3: invokevirtual producer chain
        try { String s = null; int n = s.concat("x").length(); System.out.println("B3 unreachable " + n); }
        catch (NullPointerException e) { System.out.println("B3: msg=[" + e.getMessage() + "]"); }
        // B4: unbox-then-call producer chain (getfield value unbox)
        // B5: getstatic producer of a java.lang static — via Boolean.FALSE then unbox
        try { boolean v = Boolean.FALSE; List<String> l = null; l.add("x"); System.out.println("B5 unreachable"); }
        catch (NullPointerException e) { System.out.println("B5: msg=[" + e.getMessage() + "]"); }
        // B6: monitorexit (synchronized on non-null then null re-enter? hard — skip)
        // B6: interface producer through checkcast into invokevirtual on String
        try { List<String> l = new ArrayList<>(); l.add(null); String s = (String) l.get(0); int n = s.length(); System.out.println("B6 unreachable " + n); }
        catch (NullPointerException e) { System.out.println("B6: msg=[" + e.getMessage() + "]"); }
        System.out.println("done");
    }
}
