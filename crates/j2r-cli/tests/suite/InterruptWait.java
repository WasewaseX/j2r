/** T14 (M4 Wave 4): Thread interrupt semantics — wait path and the
 * monitor-enter immunity.
 *
 * Pinned against Temurin 21.0.13 by InterruptProbe / EdgeProbe:
 * `Object.wait` interrupted throws a MESSAGE-LESS InterruptedException
 * AFTER re-acquiring the monitor (so the catch handler still owns it and
 * notify() inside the catch succeeds); a pre-set flag makes wait throw
 * immediately without releasing; a thread interrupted while BLOCKED ON
 * MONITOR-ENTER never throws, enters normally, and keeps the flag set.
 *
 * Schedule-robust (T17/gate-13, the DaemonInterrupt pattern): in coop
 * mode sleep is a yield, so the part-(c) holder would release the lock
 * before the contender ever blocks on the enter. The holder instead
 * keeps the lock until main releases it (volatile done flag, spin
 * inside the monitor), and the contender only attempts the enter after
 * the holder PROVABLY owns it. At interrupt time the contender is
 * either still outside the enter (pre-set flag -> immune enter, sticky
 * flag) or parked inside it (immunity, sticky flag) — the same line
 * either way.
 */
public class InterruptWait {
    static final Object lock = new Object();
    static volatile String m = "none";
    static volatile boolean holding = false;
    static volatile boolean arrived = false;
    static volatile boolean done = false;

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
        // re-acquired before delivery. Seed-robust without a handshake:
        // a pre-set flag (interrupt before the wait) and a delivered
        // one (during the wait) print the same line.
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
        // interrupted while blocked on (or just before) the enter,
        // enters normally, and the flag survives (sticky).
        Thread holder = new Thread(() -> {
            synchronized (lock) {
                holding = true;
                while (!done) {
                    Thread.yield();
                }
            }
        });
        Thread contender = new Thread(() -> {
            while (!holding) {
                Thread.yield();
            }
            arrived = true;
            synchronized (lock) {
                m = "entered flag=" + Thread.currentThread().isInterrupted();
            }
        });
        holder.start();
        while (!holding) {
            Thread.sleep(5);
        }
        contender.start();
        while (!arrived) {
            Thread.sleep(5);
        }
        contender.interrupt();
        done = true;
        holder.join();
        contender.join();
        System.out.println("enter=" + m);
    }
}
