import java.lang.reflect.Constructor;

public class NewProbe {
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

    public static void main(String[] args) throws Exception {
        // P1: newInstance RUNS the clinit (once) and the ctor sees the
        // JLS default (x=0) before assignment.
        Class<?> c = Class.forName("NewProbe$Plain");
        Constructor<?> k = c.getDeclaredConstructor();
        k.setAccessible(true);
        Object o = k.newInstance();
        System.out.println("--");
        // P2: a second newInstance does NOT re-run the clinit.
        k.newInstance();
        // P3: ctor args flow.
        Constructor<?> k2 = Class.forName("NewProbe$WithArgs").getDeclaredConstructor(String.class);
        k2.setAccessible(true);
        k2.newInstance("hello");
        // P4: the errant-clinit class — the NCDFE law through newInstance.
        try {
            Class.forName("NewProbe$Errant");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // P5: a private ctor of the SAME package succeeds (the
        // caller-sensitivity residual — no IllegalAccessException).
        Constructor<?> k3 = Class.forName("NewProbe$Private").getDeclaredConstructor();
        k3.setAccessible(true);
        k3.newInstance();
        // P6: an abstract class throws InstantiationException.
        Constructor<?> k4 = Class.forName("NewProbe$Abstract").getDeclaredConstructor();
        k4.setAccessible(true);
        try {
            k4.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
    }
}
