package pA;

/** The different-package private-ctor probe target (top-level, so NO
 *  nestmate outside pA$* exists). */
public class Private {
    private Private() {
        System.out.println("private<init> ran");
    }
    public static void touch() {
        System.out.println("pA.Private.touch ran");
    }
}
