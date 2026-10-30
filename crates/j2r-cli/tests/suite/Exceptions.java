class TooSmallException extends Exception {
    TooSmallException(String msg) { super(msg); }
}

class TooBigException extends Exception {
    TooBigException(String msg) { super(msg); }
}

public class Exceptions {
    static void check(int v) throws TooSmallException, TooBigException {
        if (v < 10) throw new TooSmallException("too small: " + v);
        if (v > 100) throw new TooBigException("too big: " + v);
        System.out.println("ok " + v);
    }

    static int risky(int a, int b) {
        return a / b;
    }

    public static void main(String[] args) {
        try {
            check(50);
            check(5);
            System.out.println("unreachable");
        } catch (TooSmallException e) {
            System.out.println("small: " + e.getMessage());
        } catch (TooBigException e) {
            System.out.println("big: " + e.getMessage());
        }
        try {
            check(500);
        } catch (Exception e) {
            System.out.println("generic: " + e.getMessage());
        }
        try {
            System.out.println(risky(1, 0));
        } catch (ArithmeticException e) {
            System.out.println("arith: " + e.getMessage());
        }
        String s = null;
        try {
            System.out.println(s.length());
        } catch (NullPointerException e) {
            System.out.println("npe caught");
        }
        // finally + rethrow
        System.out.println(finallyTest());
        try {
            inner(true);
        } catch (IllegalStateException e) {
            System.out.println("rethrown: " + e.getMessage());
        }
        // multi-catch
        try {
            if (args.length == -1) throw new TooSmallException("x");
            else throw new TooBigException("y");
        } catch (TooSmallException | TooBigException e) {
            System.out.println("multi: " + e.getMessage());
        }
        // exception in callee, caught by caller
        try {
            riskyThrow();
        } catch (ArithmeticException e) {
            System.out.println("from callee");
        }
        System.out.println("end");
    }

    static int finallyTest() {
        try {
            return risky(6, 2);
        } finally {
            System.out.println("finally runs");
        }
    }

    static void inner(boolean boom) {
        try {
            if (boom) throw new IllegalStateException("inner");
            System.out.println("no boom");
        } catch (IllegalStateException e) {
            System.out.println("caught in inner");
            throw e;
        }
    }

    static void riskyThrow() {
        int[] z = new int[1];
        System.out.println(z[5]);
    }
}
