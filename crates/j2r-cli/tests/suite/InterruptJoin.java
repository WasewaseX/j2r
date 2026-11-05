/** T14 (M4 Wave 4): Thread interrupt semantics — join path.
 *
 * Pinned against Temurin 21.0.13 by JoinInterruptProbe: an interrupted
 * join (with or without a timeout) throws a MESSAGE-LESS
 * InterruptedException, including the self-join case (blocks forever
 * until interrupted, like the JVM).
 *
 * Schedule-robust (T17/gate-13, the DaemonInterrupt pattern): in coop
 * mode sleep is a yield, so a sleeping target would die long before the
 * joiner joins it and the IE could never be delivered. The target
 * instead stays alive by spinning on its own interrupt flag — one
 * mechanism for both schedules (free mode: the spin ends exactly when
 * the delivered interrupt sets the flag; coop mode: same). The cross-
 * thread part is ordered by a volatile handshake, and the two possible
 * interleavings print the same line: the interrupt lands after the
 * joiner parked in target.join() (delivered IE) or before it reached
 * the join (pre-set flag -> immediate IE) — message-less either way.
 */
public class InterruptJoin {
    static volatile String m = "none";
    static volatile boolean joining = false;

    static void joinSelf() throws InterruptedException {
        Thread.currentThread().join();
    }

    public static void main(String[] args) throws Exception {
        // (a) self-join with a pre-set flag: immediate message-less IE.
        Thread.currentThread().interrupt();
        try {
            joinSelf();
            m = "no-throw";
        } catch (InterruptedException e) {
            m = String.valueOf(e.getMessage());
        }
        System.out.println("self=" + m + " cleared="
                + !Thread.currentThread().isInterrupted());

        // (b) cross-thread: the joiner announces itself (joining=true)
        // right before target.join(); main interrupts once the flag is
        // visible; the target is released the same way and reaped.
        Thread target = new Thread(() -> {
            while (!Thread.currentThread().isInterrupted()) {
                Thread.yield();
            }
        });
        Thread joiner = new Thread(() -> {
            joining = true;
            try {
                target.join();
                m = "no-throw";
            } catch (InterruptedException e) {
                m = String.valueOf(e.getMessage());
            }
        });
        target.start();
        joiner.start();
        while (!joining) {
            Thread.sleep(5);
        }
        joiner.interrupt();
        joiner.join();
        System.out.println("joiner=" + m);
        target.interrupt();
        target.join();
        System.out.println("target alive=" + target.isAlive());
    }
}
