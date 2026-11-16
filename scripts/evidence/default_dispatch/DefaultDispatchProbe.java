/**
 * DefaultDispatchProbe — the user-interface DEFAULT-method dispatch law
 * (the registered Probe5 residual, run 3 of the standing cycle). Temurin
 * 21.0.13+9 is the oracle; j2r pre-fix dies the loud internal Error on the
 * first default dispatch (invoke_virtual walks parents[0] only — interface
 * defaults are off the chain).
 *
 * Pinned laws:
 * - L1: the Probe5 shape — a default method calling an abstract sibling
 *   runs through the interface-typed receiver (Temurin: twice=42).
 * - L2: the MOST-SPECIFIC default wins — Mid extends Top redeclares the
 *   default; an implementor of Mid dispatched through a Top-typed site
 *   runs Mid's body.
 * - L3: the class's own override beats the default (the concrete chain
 *   walk first — pre-existing, must stay green).
 * - L4: a super-interface default is reachable through a sub-interface
 *   that does NOT redeclare it (the closure walk).
 * The diamond-conflict and re-abstract shapes are separate mechanisms
 * (class PREPARATION law, not dispatch) — DiamondConflictProbe.java.
 */
public class DefaultDispatchProbe {

    // L1: the registered Probe5 shape.
    interface WithDefault {
        int m();
        default int twice() { return m() * 2; }
    }
    static class Impl implements WithDefault {
        public int m() { return 21; }
    }

    // L2: the most-specific default wins.
    interface Top { default String t() { return "top"; } }
    interface Mid extends Top { default String t() { return "mid"; } }
    static class ImplMid implements Mid { }

    // L3: the class override beats the default.
    interface Ov { default String o() { return "default"; } }
    static class ImplOv implements Ov {
        public String o() { return "class"; }
    }

    // L4: the super-interface default through a non-redeclaring Mid.
    interface Top2 { default String t2() { return "top2"; } }
    interface Mid2 extends Top2 { }
    static class ImplMid2 implements Mid2 { }

    public static void main(String[] args) {
        WithDefault w = new Impl();
        System.out.println("m=" + w.m());
        System.out.println("twice=" + w.twice());
        Top t = new ImplMid();
        System.out.println("t=" + t.t());
        Ov o = new ImplOv();
        System.out.println("o=" + o.o());
        Mid2 m2 = new ImplMid2();
        System.out.println("t2=" + m2.t2());
    }
}
