public class MathExact {
    public static void main(String[] args) {
        System.out.println(Math.floorDiv(7, 2));
        System.out.println(Math.floorDiv(-7, 2));
        System.out.println(Math.floorDiv(7, -2));
        System.out.println(Math.floorMod(7, 2));
        System.out.println(Math.floorMod(-7, 2));
        System.out.println(Math.floorMod(7, -2));
        System.out.println(Math.floorDiv(-7, -2));
        System.out.println(Math.floorMod(-7, -2));
        System.out.println(Math.floorDiv(5L, 3L));
        System.out.println(Math.floorMod(-5L, 3L));
        System.out.println(Math.round(2.5));
        System.out.println(Math.round(-2.5));
        System.out.println(Math.round(2.5f));
        System.out.println(Math.round(0.5));
        System.out.println(Math.sqrt(2.0));
        System.out.println(Math.sqrt(16.0));
        System.out.println(Math.cbrt(27.0));
        System.out.println(Math.pow(2.0, 10.0));
        System.out.println(Math.pow(2.0, 0.5));
        System.out.println(Math.floor(2.7));
        System.out.println(Math.floor(-2.7));
        System.out.println(Math.ceil(2.3));
        System.out.println(Math.ceil(-2.3));
        System.out.println(Math.signum(3.5));
        System.out.println(Math.signum(-3.5));
        System.out.println(Math.signum(0.0));
        System.out.println(Math.signum(-0.0));
        System.out.println(1.0 / Math.signum(-0.0));
        System.out.println(Math.toIntExact(42L));
        try { Math.toIntExact(Long.MAX_VALUE); } catch (ArithmeticException e) { System.out.println("e1:" + e.getMessage()); }
        try { Math.addExact(Integer.MAX_VALUE, 1); } catch (ArithmeticException e) { System.out.println("e2:" + e.getMessage()); }
        try { Math.subtractExact(Integer.MIN_VALUE, 1); } catch (ArithmeticException e) { System.out.println("e3:" + e.getMessage()); }
        try { Math.multiplyExact(1 << 30, 4); } catch (ArithmeticException e) { System.out.println("e4:" + e.getMessage()); }
        try { Math.addExact(Long.MAX_VALUE, 1); } catch (ArithmeticException e) { System.out.println("e5:" + e.getMessage()); }
        try { Math.multiplyExact(Long.MAX_VALUE, 2); } catch (ArithmeticException e) { System.out.println("e6:" + e.getMessage()); }
        try { Math.floorDiv(1, 0); } catch (ArithmeticException e) { System.out.println("e7:" + e.getMessage()); }
        try { Math.floorMod(1, 0); } catch (ArithmeticException e) { System.out.println("e8:" + e.getMessage()); }
        System.out.println(Math.floorDiv(Integer.MIN_VALUE, -1));
        // Was an uncaught throw (pinned as a byte-identical crash via the
        // normalized-stderr channel); caught now so the (long,int) rows
        // below actually run, with the overflow MESSAGE still pinned — on
        // stdout, which is stricter than the frame-normalized stderr pin.
        try { Math.multiplyExact(Integer.MIN_VALUE, -1); } catch (ArithmeticException e) { System.out.println("e11:" + e.getMessage()); }
        // ---- the JDK 9+ mixed overloads: floorDiv(long, int) /
        // floorMod(long, int). OpenJDK widens the int divisor to long and
        // delegates to the (long, long) form; the result is a long. Rows:
        // mixed signs both ways, the negative-divisor floor law,
        // Long.MIN_VALUE numerator, the -1 divisor (floorDiv(INT_MIN-like
        // edge is unrepresentable for long MIN here, but the plain
        // identity rows pin the widen), the zero-divisor message, and
        // the floorMod identity x - floorDiv(x,y)*y == floorMod(x,y).
        System.out.println(Math.floorDiv(7L, 2));
        System.out.println(Math.floorDiv(-7L, 2));
        System.out.println(Math.floorDiv(7L, -2));
        System.out.println(Math.floorDiv(-7L, -2));
        System.out.println(Math.floorMod(7L, 2));
        System.out.println(Math.floorMod(-7L, 2));
        System.out.println(Math.floorMod(7L, -2));
        System.out.println(Math.floorMod(-7L, -2));
        System.out.println(Math.floorDiv(Long.MIN_VALUE, 2));
        System.out.println(Math.floorMod(Long.MIN_VALUE, 2));
        System.out.println(Math.floorDiv(Long.MIN_VALUE, -1));
        System.out.println(Math.floorDiv(Long.MAX_VALUE, Integer.MIN_VALUE));
        System.out.println(Math.floorMod(Long.MAX_VALUE, Integer.MIN_VALUE));
        System.out.println(Math.floorDiv(0L, -3));
        System.out.println(Math.floorMod(0L, -3));
        System.out.println(Math.floorMod(-1L, Integer.MAX_VALUE));
        System.out.println(Math.floorDiv(-1L, Integer.MAX_VALUE));
        try { Math.floorDiv(1L, 0); } catch (ArithmeticException e) { System.out.println("e9:" + e.getMessage()); }
        try { Math.floorMod(1L, 0); } catch (ArithmeticException e) { System.out.println("e10:" + e.getMessage()); }
        long x = -9034567890123L; int y = 7919;
        System.out.println(Math.floorDiv(x, y) * y + Math.floorMod(x, y) == x);
    }
}
