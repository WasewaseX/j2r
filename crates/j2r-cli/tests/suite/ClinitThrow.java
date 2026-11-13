// CLINIT-THROW differential fixture: the JLS 12.4.2 steps 11-12 machinery
// for a `<clinit>` that completes abruptly. Five shapes, each hit twice
// (the first access = the thrower, the second = the errant law):
//   rt:  a non-Error from the clinit — the first thrower sees a fresh
//        ExceptionInInitializerError whose message is null and whose cause
//        is the original object (the JDK's EIIE(Throwable) ctor shape);
//        every later first-use sees NoClassDefFoundError
//        "Could not initialize class <binary name>" whose cause is a fresh
//        EIIE carrying "Exception <E.toString()> [in thread \"main\"]"
//        with getException()/getCause() both null.
//   err: an Error subclass from the clinit — RETHROWN AS-IS (JLS 12.4.2
//        step 11 rethrows Errors unwrapped; EIIE itself is an Error
//        subclass), then the errant law.
//   eiie:an EIIE thrown directly — rethrown as-is, then the errant law.
//   blk: the static-initializer-block form (javac forbids a block that
//        cannot complete normally, so the throw rides a non-constant
//        condition).
// Oracle: Temurin 21.0.13, ClinitProbe/ClinitProbe2 captures
// (scripts/evidence, session 19b cont. 6).
public class ClinitThrow {
    static class ThrowsRuntime {
        static int v = init();
        static int init() { throw new IllegalStateException("boom-rt"); }
    }
    static class ThrowsError {
        static int v = init();
        static int init() { throw new AssertionError("boom-err"); }
    }
    static class ThrowsEIIE {
        static int v = init();
        static int init() { throw new ExceptionInInitializerError("direct"); }
    }
    static RuntimeException throwIt(String m) { return new RuntimeException(m); }
    static boolean FLAG = true; // non-constant so the block compiles
    static class StaticBlockThrow {
        static int v;
        static { if (FLAG) throw throwIt("block-throw"); }
    }

    static void hit(String label, Runnable r) {
        try {
            r.run();
            System.out.println(label + ": NO THROW");
        } catch (Throwable t) {
            System.out.println(label + ": " + t.getClass().getName() + " msg=" + t.getMessage());
            Throwable c = t.getCause();
            System.out.println(label + "-c1: " + (c == null ? "null"
                : c.getClass().getName() + " msg=" + c.getMessage()));
        }
    }

    public static void main(String[] args) {
        hit("rt-1", () -> ThrowsRuntime.v += 1);
        hit("rt-2", () -> ThrowsRuntime.v += 1);
        hit("rt-3", () -> System.out.println("rt-use=" + ThrowsRuntime.v));
        System.out.println("--");
        hit("err-1", () -> ThrowsError.v += 1);
        hit("err-2", () -> ThrowsError.v += 1);
        System.out.println("--");
        hit("eiie-1", () -> ThrowsEIIE.v += 1);
        hit("eiie-2", () -> ThrowsEIIE.v += 1);
        System.out.println("--");
        hit("blk-1", () -> StaticBlockThrow.v += 1);
        hit("blk-2", () -> StaticBlockThrow.v += 1);
        System.out.println("ClinitThrow done");
    }
}
