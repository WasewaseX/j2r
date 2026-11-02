public class SyncUnwind {
    static Object lock = new Object();
    static int held = 0;

    static int tryInside(boolean blow) {
        synchronized (lock) {
            held += 1;
            if (blow) {
                throw new RuntimeException("boom");
            }
            return held;
        }
    }

    synchronized int blowInsideMethod() {
        // the exception leaves the method still holding the monitor; the
        // frame's exit path must release it
        throw new IllegalStateException("sync-method-boom");
    }

    public static void main(String[] args) {
        int caught = 0;
        try {
            tryInside(true);
        } catch (RuntimeException e) {
            caught = 1;
        }
        System.out.println(caught);
        // the monitor was released by the unwound block's structured exit:
        // re-acquiring and completing works
        System.out.println(tryInside(false));
        System.out.println(held);

        SyncUnwind o = new SyncUnwind();
        int caught2 = 0;
        try {
            o.blowInsideMethod();
        } catch (IllegalStateException e) {
            caught2 = 1;
        }
        System.out.println(caught2);
        // re-acquiring after the exceptional method exit works
        System.out.println(o.blowInsideMethodSafe());
    }

    synchronized int blowInsideMethodSafe() {
        return 42;
    }
}
