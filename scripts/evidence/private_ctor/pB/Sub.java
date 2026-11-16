package pB;

import pA.Prot;

/** The protected-subclass caller: a different-package SUBCLASS of the
 *  declaring class — the protected law (probe L7). */
public class Sub {
    public static void run() throws Exception {
        try {
            Class.forName("pA.Prot").getDeclaredConstructor().newInstance();
            System.out.println("S4: subclass protected OK");
        } catch (Throwable t) {
            System.out.println("S4: caught " + t.getClass().getName() + ": " + t.getMessage());
        }
    }
}
