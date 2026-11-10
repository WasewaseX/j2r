public class Att2 {
    static { System.loadLibrary("att2"); }
    private static native int probe();
    public static void main(String[] a) { System.out.println("V=" + probe()); }
}
