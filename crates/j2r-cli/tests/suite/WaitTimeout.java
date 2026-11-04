public class WaitTimeout {
    static Object tap = new Object();
    static boolean tapped = false;

    public static void main(String[] args) throws Exception {
        // 1) argument validation, JDK-exact messages (owner first, then
        // ranges — so these all run while owning the monitor).
        synchronized (tap) {
            try {
                tap.wait(-1);
            } catch (IllegalArgumentException e) {
                System.out.println(e.getMessage());
            }
            try {
                tap.wait(1, -1);
            } catch (IllegalArgumentException e) {
                System.out.println(e.getMessage());
            }
            try {
                tap.wait(1, 1000000);
            } catch (IllegalArgumentException e) {
                System.out.println(e.getMessage());
            }
        }

        // 2) The RANGES are checked before ownership (the JDK range-checks
        // in the Java-level wrapper, before the native runs — pinned on
        // Temurin 21): a non-owner with a negative timeout gets the IAE,
        // and only valid args reach the ownership IMSE.
        try {
            tap.wait(-1);
        } catch (IllegalArgumentException e) {
            System.out.println("nonowner-neg: " + e.getMessage());
        }
        try {
            tap.wait(1);
        } catch (IllegalMonitorStateException e) {
            System.out.println("nonowner-valid: " + e.getMessage());
        }

        // 3) plain timed wait returns normally on expiry; measure with
        // nanoTime (millis granularity too coarse to pin 3ms).
        long t0 = System.nanoTime();
        synchronized (tap) {
            tap.wait(3);
        }
        long dt3 = (System.nanoTime() - t0) / 1000000L;
        System.out.println(dt3 >= 3 ? "slow-ok" : "fast-bad");

        // 4) nanos rounding: wait(1, 999999) budgets 2ms (1ms + 999999ns
        // rounds up), so the elapsed floor is 2ms on both the JVM and j2r.
        long t1 = System.nanoTime();
        synchronized (tap) {
            tap.wait(1, 999999);
        }
        long dtR = (System.nanoTime() - t1) / 1000000L;
        System.out.println(dtR >= 2 ? "round-ok" : "round-bad");

        // 5) notify beats the deadline: bounded wait slices with a latched
        // flag — deterministic outcome regardless of who wins the race.
        Thread n = new Thread(() -> {
            try {
                Thread.sleep(2);
            } catch (InterruptedException e) {
            }
            synchronized (tap) {
                tapped = true;
                tap.notify();
            }
        });
        n.start();
        synchronized (tap) {
            long end = System.currentTimeMillis() + 5000;
            while (!tapped) {
                tap.wait(100);
                if (System.currentTimeMillis() > end) {
                    break;
                }
            }
        }
        n.join();
        System.out.println(tapped ? "tapped" : "missed");

        System.out.println("done");
    }
}
