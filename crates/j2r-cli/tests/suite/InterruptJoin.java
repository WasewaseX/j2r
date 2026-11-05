/** T14 (M4 Wave 4): Thread interrupt semantics — join path.
 *
 * Pinned against Temurin 21.0.13 by JoinInterruptProbe: an interrupted
 * join (with or without a timeout) throws a MESSAGE-LESS
 * InterruptedException, including the self-join case (blocks forever
 * until interrupted, like the JVM).
 */
public class InterruptJoin {
    static volatile String m = "none";

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

        // (b) cross-thread: the joiner blocks on a live target, the
        // interrupt delivers the message-less IE; then the target is
        // released the same way and reaped.
        Thread target = new Thread(() -> {
            try {
                Thread.sleep(60_000);
            } catch (InterruptedException e) { /* released by main */ }
        });
        Thread joiner = new Thread(() -> {
            try {
                target.join();
                m = "no-throw";
            } catch (InterruptedException e) {
                m = String.valueOf(e.getMessage());
            }
        });
        target.start();
        Thread.sleep(100);
        joiner.start();
        Thread.sleep(100);
        joiner.interrupt();
        joiner.join();
        System.out.println("joiner=" + m);
        target.interrupt();
        target.join();
        System.out.println("target alive=" + target.isAlive());
    }
}
