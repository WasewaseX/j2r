public class ZeroInit {
    // statics without initializers: javac emits no <clinit>, the JVM still
    // gives them their default values (JVMS §4.12.4 / JLS §4.12.5)
    static int noInit;
    static long lNoInit;
    static double dNoInit;
    static boolean zNoInit;
    static String sNoInit;
    static int[] arrNoInit;
    static int withInit = 7;

    public static void main(String[] args) {
        System.out.println(noInit);
        System.out.println(lNoInit);
        System.out.println(dNoInit);
        System.out.println(zNoInit);
        System.out.println(sNoInit == null);
        System.out.println(arrNoInit == null);
        System.out.println(withInit);
        // defaults persist through reads before any write
        noInit += 3;
        System.out.println(noInit);
    }
}
