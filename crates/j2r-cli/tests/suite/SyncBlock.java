public class SyncBlock {
    private static final Object GATE = new Object();
    private int v = 0;

    int increment() {
        synchronized (this) {
            v += 1;
            return v;
        }
    }

    int nested() {
        synchronized (this) {
            synchronized (this) { // re-entrant block on the same monitor
                v += 10;
            }
            return v;
        }
    }

    int twoLocks(SyncBlock other) {
        synchronized (this) {
            synchronized (other) { // distinct monitors, deterministic order
                return v + other.v;
            }
        }
    }

    int gated() {
        synchronized (GATE) { // monitor on a static-field object
            return v * 2;
        }
    }

    public static void main(String[] args) {
        SyncBlock a = new SyncBlock();
        SyncBlock b = new SyncBlock();
        System.out.println(a.increment());
        System.out.println(a.nested());
        System.out.println(a.twoLocks(b));
        System.out.println(a.gated());
        System.out.println(b.increment());
    }
}
