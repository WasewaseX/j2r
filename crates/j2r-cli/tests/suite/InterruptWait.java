/** T14 (M4 Wave 4): Thread interrupt semantics — wait path and the
 * monitor-enter immunity.
 *
 * Pinned against Temurin 21.0.13 by InterruptProbe / EdgeProbe:
 * `Object.wait` interrupted throws a MESSAGE-LESS InterruptedException
 * AFTER re-acquiring the monitor (so the catch handler still owns it and
 * notify() inside the catch succeeds); a pre-set flag makes wait throw
 * immediately without releasing; a thread interrupted while BLOCKED ON
 * MONITOR-ENTER never throws, enters normally, and keeps the flag set.
 */
public class InterruptWait {
    static final Object lock = new Object();
    static volatile String m = "none";

    public static void main(String[] args) throws Exception {
        // (a) pre-interrupted wait: immediate throw, monitor never
        // released — notify() inside the catch must succeed.
        Thread a = new Thread(() -> {
            Thread.currentThread().interrupt();
            synchronized (lock) {
                try {
                    lock.wait();
                    m = "no-throw";
                } catch (InterruptedException e) {
                    try {
                        lock.notify();
                        m = String.valueOf(e.getMessage()) + " notified-in-catch";
                    } catch (IllegalMonitorStateException imse) {
                        m = "NOT-HELD";
                    }
                }
            }
        });
        a.start();
        a.join();
        System.out.println("pre=" + m);

        // (b) interrupt during wait: same message-less IE, monitor
        // re-acquired before delivery.
        Thread b = new Thread(() -> {
            synchronized (lock) {
                try {
                    lock.wait();
                    m = "no-throw";
                } catch (InterruptedException e) {
                    try {
                        lock.notify();
                        m = String.valueOf(e.getMessage()) + " notified-in-catch";
                    } catch (IllegalMonitorStateException imse) {
                        m = "NOT-HELD";
                    }
                }
            }
        });
        b.start();
        Thread.sleep(100);
        b.interrupt();
        b.join();
        System.out.println("during=" + m);

        // (c) monitor-enter is NOT interruptible: the contender is
        // interrupted while blocked on the enter, enters normally, and
        // the flag survives (sticky).
        Thread holder = new Thread(() -> {
            synchronized (lock) {
                try {
                    Thread.sleep(300);
                } catch (InterruptedException e) { /* irrelevant */ }
            }
        });
        Thread contender = new Thread(() -> {
            synchronized (lock) {
                m = "entered flag=" + Thread.currentThread().isInterrupted();
            }
        });
        holder.start();
        Thread.sleep(50);
        contender.start();
        Thread.sleep(100);
        contender.interrupt();
        holder.join();
        contender.join();
        System.out.println("enter=" + m);
    }
}
