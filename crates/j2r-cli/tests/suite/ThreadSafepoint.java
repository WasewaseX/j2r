public class ThreadSafepoint {
    static long total;

    public static void main(String[] args) throws Exception {
        // Allocator thread: fresh objects at every step — every iteration
        // rides a gc_point (the runtime's safepoint poll + collection
        // decision), so this thread parks/resumes through every STW round.
        Thread alloc = new Thread(() -> {
            long live = 0;
            for (int i = 0; i < 2000; i++) {
                Object o = new Object();
                if (o != null) {
                    live++;
                }
            }
            total = live;
        });
        // Churn thread: string concatenation garbage — drives collections
        // from a second thread against the allocator's.
        Thread churn = new Thread(() -> {
            for (int i = 0; i < 500; i++) {
                String s = "chunk-" + i + "-" + (i * 7);
                if (s.length() < 0) {
                    System.out.println("impossible");
                }
            }
        });
        alloc.start();
        churn.start();
        alloc.join();
        churn.join();
        // All printing on main AFTER the joins: byte-identical output on
        // any scheduler. The join edges also publish the child's
        // non-volatile static write (JMM start/join happens-before).
        System.out.println("alloc total " + total);
        System.out.println("done");
    }
}
