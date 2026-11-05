public class HexFloat {
    // Deterministic bit-printing helpers: floatToIntBits / doubleToLongBits
    // collapse NaNs and expose exact IEEE patterns, so the comparison is
    // bit-exact on every channel (stdout only integers + exceptions).
    static void f(String in) {
        try {
            float v = Float.parseFloat(in);
            System.out.println("f " + in + " => " + Integer.toHexString(Float.floatToIntBits(v)));
        } catch (NumberFormatException e) {
            System.out.println("f " + in + " !! " + e.getMessage());
        }
    }
    static void d(String in) {
        try {
            double v = Double.parseDouble(in);
            System.out.println("d " + in + " => " + Long.toHexString(Double.doubleToLongBits(v)));
        } catch (NumberFormatException e) {
            System.out.println("d " + in + " !! " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        // basic grammar shapes (Temurin-pinned via corpus/HexFloatProbe.java)
        f("0x1.8p1");
        d("0x1.8p1");
        f("0X1.8P1");
        f("0x.8p1");
        f("0x1.p1");
        f("0x1p1");
        d("0x1P1");
        f("-0x1.8p1");
        f("+0x1.8p1");
        d("-0x1p1");
        // suffixes
        f("0x1.8p1f");
        f("0x1.8p1F");
        f("0x1.8p1d");
        d("0x1.8p1D");
        // exponent signs / zero-padded exponents
        f("0x1p-1");
        f("0x1p+1");
        d("0x1p-1");
        d("0x1p+1");
        f("0x1p01");
        d("0x1.8p+0");
        // signed zeros (the sign of zero is observable)
        f("0x0p0");
        f("-0x0p0");
        f("+0x0p0");
        d("-0x0.0p0");
        f("0x0p-149");
        d("-0x0p-1074");
        // no 0x prefix → NumberFormatException (Javadoc ". HexDigits" branch NOT accepted)
        f(".8p1");
        d(".8p1");
        f("8p1");
        f("1p1");
        // missing pieces
        f("0x1.8");
        d("0x1.8");
        f("0x1p");
        f("0xp1");
        f("0x");
        d("0x.8");
        d("0x1.8p");
        f("0x1.8p1.5");
        f("0x1.8px1");
        f("0x1.8p 1");
        f(" 0x1.8p1 ");
        d("0x1.8p1 ");
        // rounding: ties to even, above/below halfway
        d("0x1.00000000000001p0");
        d("0x1.00000000000008p0");
        d("0x1.00000000000009p0");
        d("0x1.0000000000000fp0");
        f("0x1.000001p0");
        f("0x1.0000008p0");
        f("0x1.0000009p0");
        f("0x1.ffffffp-1");
        f("0x1.fffffep-1");
        // overflow: exact max-finite boundary and the half-ulp tie → Infinity
        f("0x1p128");
        f("0x1p127");
        f("0x1.fffffep127");
        f("0x1.ffffffp127");
        f("-0x1p128");
        d("0x1p1024");
        d("0x1p1023");
        d("0x1.fffffffffffffp1023");
        d("0x1.ffffffffffff8p1023");
        d("-0x1p2000");
        // subnormals: grid, ties, round-up crossings
        f("0x1p-149");
        f("0x1p-150");
        f("0x1.000001p-149");
        f("0x1.8p-150");
        f("0x1p-126");
        f("0x1p-127");
        d("0x1p-1074");
        d("0x1p-1075");
        d("0x1.8p-1075");
        d("0x1p-1022");
        d("0x1p-1023");
        d("0x1.fffffffffffffp-1022");
        d("-0x1p-1074");
        d("0x1p-3000");
        f("0x1p-3000");
        // long significands: trim + sticky, deep fractions
        d("0x0.00000000000000000000000000001p0");
        d("0x1.0000000000000000000001p0");
        f("0x1.0000000000000000000001p0");
        d("0x123456789abcdefp0");
        f("0x123456789abcdefp0");
        d("0xfedcba987654321p-10");
        f("0x0000000000000000000001p3");
        // 'e' is a hex digit, not an exponent marker
        f("0x1.8e2p1");
        f("0x1.8p1e2");
        d("0x0000p-5");
        // garbage
        f("0xg1p1");
        f("0x1.8.p1");
        f("0x1..8p1");
        f("0x1.8p1x");
        f("xx");
        f("0b101");
        // decimal forms unchanged (regression)
        f("2.5");
        d("1e10");
        f("Infinity");
        d("NaN");
        f("inf");
        f("");
        System.out.println("done");
    }
}
