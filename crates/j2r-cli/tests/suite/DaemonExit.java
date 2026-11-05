/** T15 (M4 Wave 4): the JVM shutdown rule — main exits when only daemons
 * remain. The daemon is started and never joined; `main` prints and
 * returns, and the process ends while the daemon is still sleeping
 * (Temurin kills daemons mid-sleep at exit — nothing after its sleep can
 * print). The only output is main's own line, so the fixture is
 * scheduling-independent in both free-running and coop modes.
 */
public class DaemonExit {
    public static void main(String[] args) throws Exception {
        Thread d = new Thread(() -> {
            try {
                Thread.sleep(60_000);
                System.out.println("daemon=ran-to-completion");
            } catch (InterruptedException e) {
                System.out.println("daemon=interrupted");
            }
        });
        d.setDaemon(true);
        d.start();
        System.out.println("main-done daemon=" + d.isAlive());
    }
}
