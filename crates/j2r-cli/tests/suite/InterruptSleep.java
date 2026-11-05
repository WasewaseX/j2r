/** T14 (M4 Wave 4): Thread interrupt semantics — sleep path.
 *
 * Pinned against Temurin 21.0.13 by InterruptProbe / EdgeProbe:
 * `sleep` interrupted carries "sleep interrupted" and CLEARS the flag;
 * a pre-set flag makes the next sleep throw immediately; the static
 * protocol reads (isInterrupted no-clear, interrupted clear) is
 * true/true/true/false; interrupt on a dead thread is a silent no-op
 * that still sets the flag (observed on 21.0.13).
 *
 * Output is scheduling-independent in free-running mode: every print
 * sits behind a join edge or a deterministic flag protocol.
 */
public class InterruptSleep {
    public static void main(String[] args) throws Exception {
        // (a) self-interrupt then sleep: immediate delivery, pinned
        // message, flag cleared by the delivery.
        Thread.currentThread().interrupt();
        String m1;
        try {
            Thread.sleep(60_000);
            m1 = "no-throw";
        } catch (InterruptedException e) {
            m1 = e.getMessage();
        }
        System.out.println("self=" + m1
                + " cleared=" + !Thread.currentThread().isInterrupted());

        // (b) flag protocol: isInterrupted does not clear, interrupted()
        // does (probe: true/true/true/false).
        Thread.currentThread().interrupt();
        boolean i1 = Thread.currentThread().isInterrupted();
        boolean i2 = Thread.currentThread().isInterrupted();
        boolean s1 = Thread.interrupted();
        boolean s2 = Thread.interrupted();
        System.out.println("flags=" + i1 + "," + i2 + "," + s1 + "," + s2);

        // (c) cross-thread delivery: the worker sleeps for a minute; the
        // interrupt wakes it with the same pinned message. The worker's
        // print happens before the join, main prints after it.
        Thread t = new Thread(() -> {
            try {
                Thread.sleep(60_000);
                System.out.println("worker=no-throw");
            } catch (InterruptedException e) {
                System.out.println("worker=" + e.getMessage()
                        + " cleared=" + !Thread.currentThread().isInterrupted());
            }
        });
        t.start();
        Thread.sleep(100);
        t.interrupt();
        t.join();
        System.out.println("joined alive=" + t.isAlive());

        // (d) interrupt after death: silent, and the flag stays readable
        // on the dead thread (Temurin 21.0.13 quirk, dead-probe).
        t.interrupt();
        System.out.println("dead=" + t.isInterrupted() + " alive=" + t.isAlive());
    }
}
