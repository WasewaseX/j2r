public class StackDepth {
    // Unbounded recursion: every JVM throws StackOverflowError at some
    // implementation-defined depth, so fixtures must never assert a depth —
    // only that the error is catchable and the thread continues afterwards.
    static int down(int n) {
        return down(n + 1) + 1;
    }
    public static void main(String[] args) {
        try {
            down(1);
            System.out.println("no-soe");
        } catch (StackOverflowError e) {
            System.out.println("caught-soe");
            System.out.println(e.getMessage() == null ? "msg-null" : "msg-present");
        }
        // The thread must keep working normally after the catch.
        System.out.println("continued");
        System.out.println("sum " + (2 + 3));
        try {
            down(1);
            System.out.println("no-soe-2");
        } catch (StackOverflowError e) {
            System.out.println("caught-again");
        }
        System.out.println("done");
    }
}
