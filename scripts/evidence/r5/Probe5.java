/**
 * Probe5 — R5 slice 2 residual registration: the user-interface DEFAULT
 * method. Temurin 21.0.13+9 runs it; j2r's dispatch walk (invoke_virtual's
 * concrete-class chain: user method per level, native row per level,
 * parent[0]) never visits the interface's own methods, so the call should
 * die the internal Error. This probe DOCUMENTS the pre-existing loud gap —
 * slice 2's fold mirrors the runtime law exactly and must not widen it.
 */
public class Probe5 {
    interface WithDefault {
        int m();
        default int twice() { return m() * 2; }
    }
    static class Impl implements WithDefault {
        public int m() { return 21; }
    }
    public static void main(String[] args) {
        WithDefault w = new Impl();
        System.out.println("m=" + w.m());
        System.out.println("twice=" + w.twice());
    }
}
