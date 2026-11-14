/**
 * FORNAME-INIT — Class.forName must RUN the &lt;clinit&gt; (the registered
 * residual, fixed 2026-11-14). The laws, probed against Temurin 21.0.13
 * (ForNameProbe) and pinned here differentially:
 *   F1  forName(name) on a never-touched class runs its clinit (the
 *       side effect prints) and a second forName does NOT re-run it;
 *   F2  forName(name, false, loader) resolves WITHOUT running it — and
 *       a later real first-use runs it exactly once;
 *   F3  a clinit that throws through forName surfaces the JLS 12.4.2
 *       EIIIE law (RuntimeException wrapped message-null);
 *   F4  every later first-use of the errant class throws NCDFE
 *       "Could not initialize class ..." with the fresh-EIIE cause
 *       carrying the original throw;
 *   F5  an Error clinit rethrows AS-IS;
 *   F6  the array descriptor resolves WITHOUT initializing the
 *       component (JLS 12.4.1);
 *   F7  an unknown name is the ClassNotFoundException with the input
 *       name verbatim.
 * The machine is the one clinit::ensure already implements for the
 * bytecode ensure sites — forName now reaches it through the class
 * metadata (id + clinit fn).
 */
public class ForNameInit {
    static class NeverTouched {
        static int side = 41;

        static {
            System.out.println("NeverTouched<clinit>");
        }
    }

    static class Lazy {
        static int side = 7;

        static {
            System.out.println("Lazy<clinit>");
        }
    }

    static class Err {
        static {
            if (System.currentTimeMillis() > 0) throw new RuntimeException("boom");
        }
    }

    static class ErrA {
        static {
            if (System.currentTimeMillis() > 0) throw new AssertionError("bang");
        }
    }

    static class Lazy2 {
        static {
            System.out.println("Lazy2<clinit>");
        }
    }

    public static void main(String[] args) throws Exception {
        // F1.
        Class.forName("ForNameInit$NeverTouched");
        System.out.println(NeverTouched.side);
        Class.forName("ForNameInit$NeverTouched");
        System.out.println(NeverTouched.side);
        // F2.
        Class<?> c = Class.forName("ForNameInit$Lazy", false, ForNameInit.class.getClassLoader());
        System.out.println("no-init ok " + (c != null));
        System.out.println(Lazy.side);
        // F3.
        try {
            Class.forName("ForNameInit$Err");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // F4.
        try {
            Class.forName("ForNameInit$Err");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println(t.getCause());
        }
        // F5.
        try {
            Class.forName("ForNameInit$ErrA");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // F6 (Lazy2's clinit must NOT print).
        Class<?> arr = Class.forName("[LForNameInit$Lazy2;");
        System.out.println(arr.getName());
        // F7.
        try {
            Class.forName("no.such.Klass");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
    }
}
