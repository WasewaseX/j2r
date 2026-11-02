public class SyncReentrant {
    static Object lock = new Object();
    static int depth = 0;
    static long checksum = 0;

    static void recurse(int n) {
        synchronized (lock) {
            depth += 1;
            if (n > 0) {
                recurse(n - 1);
            } else {
                checksum += depth; // full re-entrancy depth at the bottom
            }
            depth -= 1;
        }
    }

    public static void main(String[] args) {
        for (int i = 0; i < 50; i++) {
            recurse(100); // 100-deep re-entrancy per call
        }
        System.out.println(depth);
        System.out.println(checksum);

        // rapid enter/exit pairs (stress: 100k acquisitions)
        long ops = 0;
        for (int i = 0; i < 100000; i++) {
            synchronized (lock) {
                ops += 1;
            }
        }
        System.out.println(ops);
    }
}
