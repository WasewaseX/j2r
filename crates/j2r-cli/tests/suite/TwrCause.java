// Audit A2 fixture: cause chain plus suppression — the primary exception
// carries a cause (constructor `Throwable(String, Throwable)`) and a
// try-with-resources close failure rides along as suppressed. Uncaught,
// so the comparison covers the JVM print order: primary header, then
// `Suppressed:` sections, then `Caused by:`.
public class TwrCause {
    static class Store implements AutoCloseable {
        private final String path;
        Store(String path) {
            this.path = path;
            System.out.println("mount " + path);
        }
        void put(String k, int v) {
            System.out.println("put " + path + " " + k + "=" + v);
        }
        @Override
        public void close() {
            System.out.println("unmount " + path);
            throw new IllegalStateException("unmount failed for " + path);
        }
    }

    public static void main(String[] args) {
        try (Store s = new Store("data.bin")) {
            s.put("alpha", 1);
            s.put("beta", 2);
            throw new RuntimeException("commit failed", new IllegalStateException("journal closed"));
        }
    }
}
