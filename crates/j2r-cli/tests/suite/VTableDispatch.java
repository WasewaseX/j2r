/**
 * VTableDispatch — R5 slice 2: the codegen-time vtable + the interface
 * closure. Pins the polymorphic residue's per-receiver direct dispatch and
 * the interface-implementor closure walk, byte-identical vs Temurin:
 *
 * - the interface receiver set is the implementor closure (sub-interfaces
 *   and super chains included; interfaces themselves are never receivers);
 * - a monomorphic interface site folds to the one implementor;
 * - a polymorphic interface site dispatches through the per-class vtable
 *   arms (one class read, one direct call per arm, the dynamic walk as
 *   the fallback);
 * - a class-typed polymorphic site gets the same vtable treatment;
 * - receivers sharing one walk target share one arm (DBase/DMid below);
 * - an inherited interface method lands on the BASE's compiled method.
 */
public class VTableDispatch {

    // The monomorphic interface: exactly one implementor in the program.
    interface Mono {
        String tag();
    }

    static class OnlyMono implements Mono {
        public String tag() { return "mono-only"; }
    }

    // The polymorphic interface: two implementors diverge.
    interface Shape {
        String name();
    }

    static class Square implements Shape {
        public String name() { return "square"; }
    }

    static class Circle implements Shape {
        public String name() { return "circle"; }
    }

    // The sub-interface closure: Mid extends Top; ViaMid implements Mid —
    // a Top-typed site must see ViaMid as a possible receiver.
    interface Top {
        int value();
    }

    interface Mid extends Top {
    }

    static class ViaMid implements Mid {
        public int value() { return 7; }
    }

    // Deep override: DBase.m is inherited by DMid (shared walk target) and
    // overridden by DSub (distinct target) — the vtable groups DBase/DMid
    // under one arm.
    interface Deep {
        String m();
    }

    static class DBase implements Deep {
        public String m() { return "dbase"; }
    }

    static class DMid extends DBase {
    }

    static class DSub extends DMid {
        public String m() { return "dsub"; }
    }

    // Class-typed polymorphism: Base/Sub both carry vf.
    static class Base {
        String vf() { return "base-vf"; }
    }

    static class Sub extends Base {
        String vf() { return "sub-vf"; }
    }

    // Inherited interface method: the walk starts at the concrete class
    // and lands on the BASE's compiled method.
    interface Inherited {
        String who();
    }

    static class InhBase implements Inherited {
        public String who() { return "inh-base"; }
    }

    static class InhSub extends InhBase {
    }

    static String render(Shape s) {
        return "shape=" + s.name();
    }

    public static void main(String[] args) {
        // L1: the monomorphic interface site — every call folds to the
        // single implementor.
        Mono mono = new OnlyMono();
        System.out.println("L1 " + mono.tag());
        System.out.println("L1b " + mono.tag());

        // L2: the polymorphic interface site — the vtable arms dispatch
        // per concrete class, in both orders.
        Shape a = new Square();
        Shape b = new Circle();
        System.out.println("L2 " + a.name() + " " + b.name());
        System.out.println("L2b " + render(b) + " " + render(a));

        // L3: the sub-interface closure through a Top-typed site.
        Top t = new ViaMid();
        System.out.println("L3 " + t.value());

        // L4: the deep-override grouping — DMid inherits DBase.m, DSub
        // overrides; three possible receivers, two arms.
        Deep d1 = new DBase();
        Deep d2 = new DMid();
        Deep d3 = new DSub();
        System.out.println("L4 " + d1.m() + " " + d2.m() + " " + d3.m());

        // L5: the class-typed polymorphic site — the Base-typed vf() call
        // dispatches through the vtable.
        Base asBase = new Sub();
        Base pure = new Base();
        System.out.println("L5 " + asBase.vf() + " " + pure.vf());

        // L6: the inherited interface method — the concrete class is
        // InhSub but the walk lands on InhBase's compiled method.
        Inherited i = new InhSub();
        System.out.println("L6 " + i.who());

        // L7: mixed arrays — receiver classes arrive through an array of
        // the interface type; the vtable still sees the concrete classes.
        Shape[] zoo = { new Circle(), new Square(), new Circle() };
        StringBuilder sb = new StringBuilder();
        for (Shape s : zoo) {
            sb.append(s.name()).append(',');
        }
        System.out.println("L7 " + sb);

        // L8: polymorphic arguments through a helper — every call site of
        // render carries the full implementor closure.
        System.out.println("L8 " + render(new Square()));
    }
}
