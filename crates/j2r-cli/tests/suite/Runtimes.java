public class Runtimes {
    public static void main(String[] args) {
        // ArithmeticException: / by zero
        try {
            int zero = args.length - args.length;
            System.out.println(10 / zero);
        } catch (ArithmeticException e) {
            System.out.println("arith: " + e.getMessage());
        }
        // ArrayIndexOutOfBoundsException with the exact OpenJDK message
        try {
            int[] a = new int[3];
            System.out.println(a[5]);
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println("oob: " + e.getMessage());
        }
        // NegativeArraySizeException
        try {
            int[] b = new int[args.length - 2];
        } catch (NegativeArraySizeException e) {
            System.out.println("neg: " + e.getMessage());
        }
        // NullPointerException (caught)
        String s = null;
        try {
            System.out.println(s.length());
        } catch (NullPointerException e) {
            System.out.println("npe caught");
        }
        // ClassCastException — only the fact is compared, not the message
        // (the exact CCE text differs between JDK versions)
        try {
            Object o = "str";
            Integer i = (Integer) o;
            System.out.println("never: " + i);
        } catch (ClassCastException e) {
            System.out.println("cce caught");
        }
        // Uncaught at the end: stdout above must match, stderr must name the
        // same exception type and message, and both must exit 1.
        int[] c = new int[1];
        System.out.println("before-uncaught");
        System.out.println(c[9]);
        System.out.println("after-uncaught");
    }
}
