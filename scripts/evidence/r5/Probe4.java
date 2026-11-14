/**
 * Probe4 — R5 slice 2: the user-interface dispatch laws, pinned vs Temurin
 * 21.0.13+9. The runtime dispatch of invokeinterface is invoke_virtual's
 * concrete-class chain walk (user method per level, native row per level,
 * parent[0]); these probes pin exactly which receiver sets and walks the
 * slice-2 interface-closure fold may mirror.
 */
public class Probe4 {
    // L1: the monomorphic interface site — ONE implementor class in the
    // whole program; every I-typed call walks to that single method.
    interface Mono { String m(); }
    static class OnlyImpl implements Mono {
        public String m() { return "only"; }
    }

    // L2: the polymorphic interface site — two implementors diverge.
    interface Poly { String m(); }
    static class P1 implements Poly {
        public String m() { return "p1"; }
    }
    static class P2 implements Poly {
        public String m() { return "p2"; }
    }

    // L3: the sub-interface closure — J extends I; an implementor of J is
    // a possible receiver of every I-typed call site.
    interface Top { String m(); }
    interface Mid extends Top {}
    static class ViaMid implements Mid {
        public String m() { return "via-mid"; }
    }

    // L4: the inherited interface method — the base class supplies m; the
    // subclass inherits it; an I-typed ref holding the subclass must walk
    // the concrete chain to the BASE's method (not the subclass).
    interface Inh { String m(); }
    static class InhBase implements Inh {
        public String m() { return "base-m"; }
    }
    static class InhSub extends InhBase {}

    // L5: the receiver identity through the interface — the walk starts at
    // the CONCRETE class even when the override sits two levels down.
    interface Deep { String m(); }
    static class DBase implements Deep {
        public String m() { return "dbase"; }
    }
    static class DMid extends DBase {}
    static class DSub extends DMid {
        @Override public String m() { return "dsub-override"; }
    }

    public static void main(String[] args) {
        Mono mono = new OnlyImpl();
        System.out.println("L1 " + mono.m());
        System.out.println("L1 again " + mono.m());

        Poly p1 = new P1();
        Poly p2 = new P2();
        System.out.println("L2 " + p1.m() + " " + p2.m());
        System.out.println("L2 poly " + ((args.length == 0 ? p1 : p2)).m());

        Top viaMid = new ViaMid();
        System.out.println("L3 " + viaMid.m());

        InhSub sub = new InhSub();
        Inh inh = sub;
        System.out.println("L4 " + inh.m());

        Deep d1 = new DBase();
        Deep d2 = new DSub();
        System.out.println("L5 " + d1.m() + " " + d2.m());

        // L6: the null-receiver law — the bare NPE before any dispatch.
        Poly nil = null;
        try {
            nil.m();
            System.out.println("L6 no throw");
        } catch (NullPointerException e) {
            String msg = e.getMessage();
            System.out.println("L6 NPE msg=" + (msg == null ? "null" : msg));
        }
    }
}
