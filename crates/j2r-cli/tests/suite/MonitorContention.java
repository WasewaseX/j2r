public class MonitorContention {
    static Object tally = new Object();
    static int count = 0;
    static int reenter = 0;
    static long seqSum = 0;

    static Object gate = new Object();
    static boolean gateOpen = false;
    static int gatePass = 0;

    static void work(int iters, int salt) {
        for (int i = 0; i < iters; i++) {
            synchronized (tally) {
                count += 1;
                // re-entrant acquisition while other threads are queued on
                // the same monitor: the count must stay per-owner
                synchronized (tally) {
                    reenter += 1;
                }
                seqSum += salt;
            }
        }
    }

    static void racer() {
        for (int i = 0; i < 500; i++) {
            synchronized (gate) {
                gatePass += 1;
            }
        }
    }

    static void sleeper() {
        synchronized (gate) {
            while (!gateOpen) {
                try {
                    gate.wait();
                } catch (InterruptedException e) {
                }
            }
            gatePass += 100000;
        }
    }

    public static void main(String[] args) throws Exception {
        // 1) four threads contending on one monitor: exact totals, plus a
        // re-entrant acquisition inside every contended critical section.
        Thread t1 = new Thread(() -> work(2500, 1));
        Thread t2 = new Thread(() -> work(2500, 2));
        Thread t3 = new Thread(() -> work(2500, 3));
        Thread t4 = new Thread(() -> work(2500, 4));
        t1.start();
        t2.start();
        t3.start();
        t4.start();
        t1.join();
        t2.join();
        t3.join();
        t4.join();
        System.out.println(count);
        System.out.println(reenter);
        System.out.println(seqSum);

        // 2) a waiter parked in the wait set while two racers hammer the
        // same monitor: the wakeup must survive the storm of enter/exit
        // cycles and re-compete for the lock like any enter-contender.
        Thread s = new Thread(() -> sleeper());
        Thread r1 = new Thread(() -> racer());
        Thread r2 = new Thread(() -> racer());
        s.start();
        Thread.sleep(50);
        r1.start();
        r2.start();
        Thread.sleep(20);
        synchronized (gate) {
            gateOpen = true;
            gate.notifyAll();
        }
        s.join();
        r1.join();
        r2.join();
        System.out.println(gatePass);

        System.out.println("done");
    }
}
