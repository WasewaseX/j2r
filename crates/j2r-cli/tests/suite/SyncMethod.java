public class SyncMethod {
    private int count = 0;
    private static int shared = 0;

    synchronized void bump(int n) {
        count += n;
    }

    // re-entrancy: a synchronized method calling synchronized methods on
    // the same monitor must not deadlock (the count is re-entrant)
    synchronized int bumpTwice(int n) {
        bump(n);
        bump(n);
        return count;
    }

    static synchronized void addShared(int n) {
        shared += n;
    }

    static synchronized void addBoth(int n) {
        addShared(n); // re-entrant on the class monitor
        shared += n * 10;
    }

    static synchronized int getShared() {
        return shared;
    }

    public static void main(String[] args) {
        SyncMethod o = new SyncMethod();
        o.bump(5);
        System.out.println(o.bumpTwice(3));
        System.out.println(o.count);

        for (int i = 0; i < 100; i++) {
            addShared(i);
        }
        addBoth(2);
        System.out.println(getShared());
    }
}
