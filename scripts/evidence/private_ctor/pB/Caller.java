package pB;

import java.lang.reflect.Constructor;

/** The different-package private-ctor matrix (the registered residual,
 *  standing since the 2026-11-14 NEW-INSTANCE slice):
 *  S1 different-package private ctor -> refused (the residual),
 *  S2 same-package non-nestmate private ctor -> refused (the nest law),
 *  S3 different-package protected ctor, unrelated caller -> refused,
 *  S4 different-package protected ctor, subclass caller -> allowed,
 *  S5 different-package public ctor -> allowed,
 *  S6 the NP3 nestmate shape (nested private class, host caller) -> allowed,
 *  S7 the raw modifier bits of the private ctor (getModifiers),
 *  S8 package-private ctor, different-package caller.
 *  Every refusal is caught and printed with the EXACT message. */
public class Caller {
    static class NestedPrivate {
        private NestedPrivate() {
            System.out.println("nested-private<init> ran");
        }
    }
    public static void main(String[] a) throws Exception {
        // S1 — the registered residual.
        try {
            Class.forName("pA.Private").getDeclaredConstructor().newInstance();
            System.out.println("S1: diff-package private OK");
        } catch (Throwable t) {
            System.out.println("S1: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // S2 — the same-package NON-nestmate law.
        pA.Same.run();
        // S3 — protected, unrelated caller.
        try {
            Class.forName("pA.Prot").getDeclaredConstructor().newInstance();
            System.out.println("S3: diff-package protected OK");
        } catch (Throwable t) {
            System.out.println("S3: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // S4 — protected, subclass caller (the instance is fresh; the
        // ctor check passes obj=null so no instance-context nuance).
        pB.Sub.run();
        // S5 — public, different package.
        try {
            Class.forName("pA.Public").getDeclaredConstructor().newInstance();
            System.out.println("S5: diff-package public OK");
        } catch (Throwable t) {
            System.out.println("S5: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // S6 — the nestmate shape (P5's actual law).
        try {
            Class.forName("pB.Caller$NestedPrivate").getDeclaredConstructor().newInstance();
            System.out.println("S6: nestmate private OK");
        } catch (Throwable t) {
            System.out.println("S6: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
        // S7 — the raw bits the check keys on.
        Constructor<?> k = Class.forName("pA.Private").getDeclaredConstructor();
        System.out.println("S7: modifiers=" + k.getModifiers());
        // S8 — package-private ctor, different-package caller.
        pB.PkgCall.run();
    }
}
