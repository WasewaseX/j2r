// Audit A2 fixture: try-with-resources where the body throws AND close()
// throws — javac's JLS 14.20.3 lowering attaches the close failure as a
// suppressed exception. Uncaught, so the comparison covers the full
// uncaught-handler output including the `Suppressed:` section.
public class TwrSuppress {
    static class Ledger implements AutoCloseable {
        private final String name;
        private final boolean failOnClose;
        Ledger(String name, boolean failOnClose) {
            this.name = name;
            this.failOnClose = failOnClose;
            System.out.println("open " + name);
        }
        void record(String entry) {
            System.out.println("record " + name + ": " + entry);
        }
        @Override
        public void close() {
            System.out.println("close " + name);
            if (failOnClose) {
                throw new IllegalStateException("close failed for " + name);
            }
        }
    }

    public static void main(String[] args) {
        try (Ledger ledger = new Ledger("tx-1", true)) {
            ledger.record("debit 41");
            ledger.record("credit 17");
            throw new RuntimeException("body failed after 2 records");
        }
    }
}
