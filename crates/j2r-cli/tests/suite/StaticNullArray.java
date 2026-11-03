public class StaticNullArray {
    // F2 pin: reference-typed statics assigned null in <clinit> — javac
    // emits aconst_null; putstatic with the array descriptor, and the
    // verifier must accept null for every reference shape, arrays
    // included. Also pins the same polymorphism at call sites (null as an
    // array-typed argument) and the null -> array transition on assignment.
    static char[] none = null;
    static int[] grid;
    static String[] names = new String[] { "a", "b" };
    static Object shared = names;

    static int firstLen(char[] cs) {
        if (cs == null) {
            return -1;
        }
        return cs.length;
    }

    public static void main(String[] args) {
        System.out.println(none == null);
        System.out.println(grid == null);
        System.out.println(none != null ? none.length : -1);
        System.out.println(firstLen(none));
        System.out.println(firstLen(new char[] { 'x', 'y', 'z' }));
        System.out.println((int[]) null == null);
        grid = new int[3];
        grid[0] = 42;
        System.out.println(grid[0] + grid.length);
        System.out.println(names.length);
        System.out.println(shared instanceof String[]);
        none = new char[] { 'q' };
        System.out.println(none.length);
        none = null;
        System.out.println(none == null);
    }
}
