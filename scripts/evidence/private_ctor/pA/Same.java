package pA;

/** The SAME-package, NON-nestmate caller (top-level class, different
 *  source file): the JDK's private law is NEST-based since JDK 11, not
 *  package-based — this caller must be REFUSED too. */
public class Same {
    public static void run() throws Exception {
        try {
            Class.forName("pA.Private").getDeclaredConstructor().newInstance();
            System.out.println("S2: same-package private OK");
        } catch (Throwable t) {
            System.out.println("S2: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
    }
}
