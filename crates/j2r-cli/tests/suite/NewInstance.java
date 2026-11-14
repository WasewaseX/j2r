import java.lang.reflect.Constructor;

/**
 * NEW-INSTANCE — Constructor.newInstance (the registered residual, fixed
 * 2026-11-14). The laws, probed against Temurin 21.0.13 (NewProbe /
 * NewProbe2 / NP3) and pinned here differentially:
 *   P1  the clinit runs before the ctor (once), and the ctor sees the
 *       JLS &sect;4.12.5 defaults (x=0 before assignment) — the layout
 *       defaults ride the class metadata;
 *   P2  a second newInstance does NOT re-run the clinit;
 *   P3  ctor arguments flow (type-checked, unmarshaled);
 *   P7  newInstance as the FIRST access of an errant-clinit class throws
 *       the BARE ExceptionInInitializerError (unwrapped) with the
 *       original as cause;
 *   P8  the second newInstance on the errant class throws the bare
 *       NoClassDefFoundError with the fresh-EIIE cause;
 *   P5  a same-package private ctor succeeds (the caller-sensitivity
 *       residual: j2r models the same-package case; the different-
 *       package IllegalAccessException is registered);
 *   P6  an abstract class throws the message-null InstantiationException;
 *   P9  a ctor's own throw wraps in InvocationTargetException with the
 *       original as cause.
 */
public class NewInstance {
    static class Plain {
        int x;

        static {
            System.out.println("Plain<clinit>");
        }

        Plain() {
            System.out.println("Plain<init> x=" + x);
            x = 5;
        }
    }

    static class WithArgs {
        String s;

        WithArgs(String s) {
            this.s = s;
            System.out.println("WithArgs<init> " + s);
        }
    }

    static class Errant {
        static {
            if (System.currentTimeMillis() > 0) throw new RuntimeException("boom");
        }

        Errant() {
            System.out.println("Errant<init>");
        }
    }

    static class Private {
        private Private() {
            System.out.println("Private<init>");
        }
    }

    static abstract class Abstract {
        Abstract() {
            System.out.println("Abstract<init>");
        }
    }

    static class CtorThrows {
        CtorThrows() {
            if (System.currentTimeMillis() > 0) throw new RuntimeException("ctor-boom");
        }
    }

    public static void main(String[] args) throws Exception {
        // P1 + P2.
        Class<?> c = Class.forName("NewInstance$Plain");
        Constructor<?> k = c.getDeclaredConstructor();
        Object o = k.newInstance();
        System.out.println("--");
        k.newInstance();
        // P3.
        Constructor<?> k2 =
                Class.forName("NewInstance$WithArgs").getDeclaredConstructor(String.class);
        k2.newInstance("hello");
        // P7.
        Constructor<?> ke = Class.forName("NewInstance$Errant", false, NewInstance.class.getClassLoader())
                .getDeclaredConstructor();
        try {
            ke.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println(t.getCause());
        }
        // P8.
        try {
            ke.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println(t.getCause());
        }
        // P5.
        Constructor<?> k3 = Class.forName("NewInstance$Private").getDeclaredConstructor();
        k3.newInstance();
        // P6.
        Constructor<?> k4 = Class.forName("NewInstance$Abstract").getDeclaredConstructor();
        try {
            k4.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // P9.
        Constructor<?> k5 = Class.forName("NewInstance$CtorThrows").getDeclaredConstructor();
        try {
            k5.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println(t.getCause());
        }
    }
}
