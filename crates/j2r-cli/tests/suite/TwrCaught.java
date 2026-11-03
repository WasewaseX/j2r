// Audit A2 fixture: caught try-with-resources failure — exercises the
// addSuppressed/getSuppressed natives through program-visible output only
// (no uncaught printing). The program inspects the suppressed chain the
// JLS 14.20.3 lowering built and prints it back out.
public class TwrCaught {
    static class Conn implements AutoCloseable {
        private final String id;
        Conn(String id) {
            this.id = id;
            System.out.println("open " + id);
        }
        void query(String q) {
            System.out.println("query " + id + " " + q);
        }
        @Override
        public void close() {
            System.out.println("close " + id);
            throw new IllegalStateException("close failed for " + id);
        }
    }

    public static void main(String[] args) {
        try (Conn c = new Conn("db0")) {
            c.query("select 1");
            throw new RuntimeException("query blew up");
        } catch (RuntimeException primary) {
            System.out.println("caught " + primary.getMessage());
            Throwable[] sup = primary.getSuppressed();
            System.out.println("suppressed count " + sup.length);
            for (Throwable s : sup) {
                System.out.println("suppressed " + s.getClass().getName() + " " + s.getMessage());
            }
            System.out.println("self-suppress blocked: " + selfSuppress(primary));
        }
    }

    static boolean selfSuppress(Throwable t) {
        try {
            t.addSuppressed(t);
            return false;
        } catch (IllegalArgumentException e) {
            return "Self-suppression not permitted".equals(e.getMessage());
        }
    }
}
