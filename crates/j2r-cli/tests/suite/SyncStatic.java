public class SyncStatic {
    static synchronized int helper(int x) {
        return x * 2;
    }

    public static void main(String[] args) {
        int total = 0;
        for (int i = 0; i < 10; i++) {
            total += helper(i); // the class monitor, acquired per call
        }
        System.out.println(total);

        // another class's static synchronized methods hold a DIFFERENT
        // monitor (synthetic per-class monitor objects)
        Other.record(7);
        System.out.println(Other.value());
    }
}

class Other {
    private static int v = 0;

    static synchronized void record(int x) {
        v = x;
    }

    static synchronized int value() {
        return v;
    }
}
