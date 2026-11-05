/** T15 (M4 Wave 4): daemon interrupt and reaping — a daemon is
 * interrupted explicitly and joined BEFORE main returns; the JVM exit
 * rule then has no live non-daemon left.
 *
 * Deterministic in both scheduler modes: in free-running mode the
 * interrupt wakes the sleeping daemon (delivered path); in coop mode the
 * daemon's sleep returns after its yield and the worker spins on the
 * interrupt flag until main's turn sets it (spin path). Both print the
 * same single line, and the join edge orders every print.
 */
public class DaemonInterrupt {
    static volatile boolean printed = false;

    public static void main(String[] args) throws Exception {
        Thread d = new Thread(() -> {
            boolean delivered = false;
            try {
                Thread.sleep(60_000);
            } catch (InterruptedException e) {
                delivered = true;
                System.out.println("daemon-interrupted");
                printed = true;
            }
            // Coop path: the sleep returned early (coop sleep is a yield,
            // not a wall-clock block); spin until the interrupt lands.
            // In free-running mode this loop is never reached (the sleep
            // threw and printed above).
            if (!delivered) {
                while (!Thread.currentThread().isInterrupted()) {
                    Thread.yield();
                }
                System.out.println("daemon-interrupted");
                printed = true;
            }
        });
        d.setDaemon(true);
        d.start();
        Thread.sleep(100);
        d.interrupt();
        d.join();
        System.out.println("reaped alive=" + d.isAlive() + " printed=" + printed);
    }
}
