/* T13 13c — the harness driver fixture (ONE source, both runtimes).
   Temurin: javac + java -Djava.library.path. j2r: j2r build + the same
   System.loadLibrary (the 13c loading story). Every probe is
   self-checking: check() exits 3 on the FIRST mismatched verdict, so a
   broken probe fails BOTH runtimes loudly instead of preserving parity
   with a wrong answer. All output goes to stdout; stderr stays empty on
   the happy path (the three-channel contract of the parity runner). */
public class HarnessDriver extends HarnessBase {
    public int instField = 41;
    public static long statField = 1000L;

    private static native int nativeVersion();
    private native long probeFields();
    private native int probeStrings(String s1, String s2);
    private native String probeStringBack();
    private native String probeUtf16();
    private native int probeArrays();
    private native String[] probeObjArray();
    private native int probeCallsA();
    private native int probeCallsV();
    private native int probeAttach();
    private native int probeRebind();
    private native int probeRestore();
    private native int probeUnregisterHelper();

    public int callback(int x) { return x * 3; }
    public String greet() { return "c-greet"; }
    public void announce(String s) { System.out.println("cb:" + s); }
    public int boom() { throw new IllegalStateException("boom-marker"); }
    public String boomMessage(Throwable t) { return t.getMessage(); }
    public static int staticHalf(int x) { return x / 2; }
    @Override
    public int baseOp(int x) { return x + 7; }
    public int vsum(int a, int b, int c, int d, int e, int f, int g, int h) {
        return a + b + c + d + e + f + g + h;
    }
    public double vmix(int a, double b, int c, double d) {
        return a + b + c + d;
    }

    private static void check(String what, long got, long want) {
        if (got != want) {
            System.out.println("FAIL " + what + " got=" + got + " want=" + want);
            System.exit(3);
        }
    }

    public static void main(String[] args) {
        System.loadLibrary("jni_harness");
        HarnessDriver d = new HarnessDriver();

        int version = d.nativeVersion();
        check("version", version, 47);
        System.out.println("P version=" + version);

        long stat = d.probeFields();
        check("fields", stat, 2000L);
        check("instField", d.instField, 42);
        check("statField", statField, 2000L);
        System.out.println("P fields=" + stat + " inst=" + d.instField
                + " stat=" + statField);

        int strings = d.probeStrings("A\u0000B", "\uD83D\uDE00");
        check("strings", strings, 3);
        System.out.println("P strings=" + strings);

        String back = d.probeStringBack();
        check("strback", back.length(), 6);
        System.out.println("P strback=" + back.length());

        String u16 = d.probeUtf16();
        check("utf16", u16.length(), 4);
        check("utf16hi", u16.charAt(0), 0xD83D);
        check("utf16lo", u16.charAt(1), 0xDE00);
        check("utf16nul", u16.charAt(2), 0);
        System.out.println("P utf16=" + u16.length() + " hi=" + (int) u16.charAt(0)
                + " lo=" + (int) u16.charAt(1) + " nul=" + (int) u16.charAt(2));

        int arrays = d.probeArrays();
        check("arrays", arrays, 63);
        System.out.println("P arrays=" + arrays);

        String[] oa = d.probeObjArray();
        System.out.println("P objarr=" + oa[0] + "," + oa[1]);

        int callsA = d.probeCallsA();
        check("callsA", callsA, 255);
        System.out.println("P callsA=" + callsA);

        int callsV = d.probeCallsV();
        check("callsV", callsV, 15);
        System.out.println("P callsV=" + callsV);

        int attach = d.probeAttach();
        check("attach", attach, 2047);
        System.out.println("P attach=" + attach);

        int rebind = d.probeRebind();
        int v2 = d.nativeVersion();
        check("rebind", rebind, 1);
        check("reboundVersion", v2, 777);
        System.out.println("P rebind=" + rebind + " v2=" + v2);

        int restore = d.probeRestore();
        int v3 = d.nativeVersion();
        check("restore", restore, 1);
        check("restoredVersion", v3, 47);
        System.out.println("P restore=" + restore + " v3=" + v3);

        HarnessHelper h = new HarnessHelper();
        System.out.println("P helper=" + h.helperAdd(4, 5));

        int unreg = d.probeUnregisterHelper();
        check("unreg", unreg, 1);
        System.out.println("P unreg=" + unreg);
        try {
            h.helperAdd(4, 5);
            System.out.println("FAIL unbound-missing");
            System.exit(3);
        } catch (UnsatisfiedLinkError e) {
            System.out.println("P unbound=" + e.getClass().getName());
        }

        System.out.println("P done");
    }
}

class HarnessBase {
    public int baseOp(int x) { return x + 1000; }
}

class HarnessHelper {
    public native int helperAdd(int a, int b);
}
