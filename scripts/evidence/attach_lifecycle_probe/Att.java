public class Att {
    static { System.loadLibrary("att"); }
    private static native int probe();
    public static void main(String[] a) { System.out.println("V=" + probe()); }
}
