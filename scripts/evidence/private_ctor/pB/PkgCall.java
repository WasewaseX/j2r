package pB;

/** S8: package-private ctor, different-package caller. */
public class PkgCall {
    public static void run() throws Exception {
        try {
            Class.forName("pA.Package").getDeclaredConstructor().newInstance();
            System.out.println("S8: diff-package package-private OK");
        } catch (Throwable t) {
            System.out.println("S8: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
    }
}
