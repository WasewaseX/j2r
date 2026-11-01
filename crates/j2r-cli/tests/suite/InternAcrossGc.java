public class InternAcrossGc {
    // Interned literals keep their identity forever (JLS 3.10.5): two ldc's
    // of the same literal are the same object, before and after
    // allocation-heavy code.
    static int sink;
    public static void main(String[] args) {
        String x = "intern-across-gc";
        String y = "intern-across-gc";
        System.out.println(x == y);
        for (int i = 0; i < 20000; i++) {
            String s = "" + i;
            sink += s.length();
        }
        String z = "intern-across-gc";
        System.out.println(x == z);
        System.out.println(("inter" + "n-across-gc") == x);
        System.out.println("sink-pos " + (sink > 0));
    }
}
