/**
 * PrivateCtorAccess — the reflection CALLER law (the different-package
 * private-ctor residual, registered 2026-11-14 at the NEW-INSTANCE
 * slice, closed 2026-11-16). The probe corpus
 * (scripts/evidence/private_ctor/, packages pA/pB) pins the WHOLE
 * matrix against live Temurin; THIS fixture pins the shapes one
 * self-contained file can reach (the suite convention is one file):
 * the nestmate private allowed (JEP 181 — the NestHost attributes
 * resolve and compare), the SAME-package NON-nestmate private refused
 * (the sharpest law: the JDK 11+ nest rule is not package-based), the
 * foreign-host nested private refused, the same-package protected and
 * package-private allowed, the public allowed, and the raw modifier
 * bits the check keys on. Every refusal is CAUGHT and printed with the
 * exact message so the differential is deterministic stdout (an
 * uncaught refusal would diverge on stderr bytes instead). The
 * diff-package refusal shapes (S1/S3/S4/S8) are pinned by the law
 * tests (caller_law_matrix / refusal_message_laws) and the probe
 * corpus's three-channel capture, not by this file.
 */
public class PrivateCtorAccess {

    static class Nested {
        private Nested() {
            System.out.println("nested-private<init> ran");
        }
    }

    public static void main(String[] args) throws Exception {
        // F1: the nestmate private (host -> its own nested class).
        try {
            Class.forName("PrivateCtorAccess$Nested").getDeclaredConstructor().newInstance();
            System.out.println("F1 nestmate OK");
        } catch (Throwable t) {
            System.out.println("F1 caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // F2: the SAME-package NON-nestmate private (Other is a
        // top-level class of this file — no shared nest) -> REFUSED.
        try {
            Class.forName("Other").getDeclaredConstructor().newInstance();
            System.out.println("F2 same-package private OK");
        } catch (Throwable t) {
            System.out.println("F2 caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // F3: a nested private of a FOREIGN host (Other$OwnNested's
        // NestHost is Other, not PrivateCtorAccess) -> REFUSED.
        try {
            Class.forName("Other$OwnNested").getDeclaredConstructor().newInstance();
            System.out.println("F3 foreign nested OK");
        } catch (Throwable t) {
            System.out.println("F3 caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // F4: same-package protected non-nestmate -> allowed.
        try {
            Class.forName("ProtectedCtor").getDeclaredConstructor().newInstance();
            System.out.println("F4 protected OK");
        } catch (Throwable t) {
            System.out.println("F4 caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // F5: same-package package-private non-nestmate -> allowed.
        try {
            Class.forName("PackageCtor").getDeclaredConstructor().newInstance();
            System.out.println("F5 package-private OK");
        } catch (Throwable t) {
            System.out.println("F5 caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // F6: public -> allowed.
        try {
            Class.forName("PublicCtor").getDeclaredConstructor().newInstance();
            System.out.println("F6 public OK");
        } catch (Throwable t) {
            System.out.println("F6 caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // F7: the raw modifier bits the law keys on.
        System.out.println("F7 private=" + Class.forName("Other").getDeclaredConstructor().getModifiers());
        System.out.println("F8 protected=" + Class.forName("ProtectedCtor").getDeclaredConstructor().getModifiers());
        System.out.println("F9 package=" + Class.forName("PackageCtor").getDeclaredConstructor().getModifiers());
        System.out.println("F10 public=" + Class.forName("PublicCtor").getDeclaredConstructor().getModifiers());
    }
}

class Other {
    private Other() {
        System.out.println("other-private<init> ran");
    }

    static class OwnNested {
        private OwnNested() {
            System.out.println("other-nested<init> ran");
        }
    }
}

class ProtectedCtor {
    protected ProtectedCtor() {
        System.out.println("protected<init> ran");
    }
}

class PackageCtor {
    PackageCtor() {
        System.out.println("package-private<init> ran");
    }
}

class PublicCtor {
    public PublicCtor() {
        System.out.println("public<init> ran");
    }
}
