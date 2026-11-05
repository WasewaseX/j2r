public class HexFloatProbe {
    static int n = 0;
    static void p(String in, boolean isFloat) {
        n++;
        String name = "case" + n + " " + in;
        try {
            if (isFloat) {
                float f = Float.parseFloat(in);
                System.out.println(name + " => " + Float.toHexString(f) + " bits=" + Integer.toHexString(Float.floatToIntBits(f)));
            } else {
                double d = Double.parseDouble(in);
                System.out.println(name + " => " + Double.toHexString(d) + " bits=" + Long.toHexString(Double.doubleToLongBits(d)));
            }
        } catch (Throwable e) {
            System.out.println(name + " !! " + e.getClass().getSimpleName() + " | " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        // basic shapes
        p("0x1.8p1", true);
        p("0x1.8p1", false);
        p("0X1.8P1", true);
        p("0x.8p1", true);
        p("0x1.p1", true);
        p("0x1p1", true);
        p("0x1P1", false);
        p("-0x1.8p1", true);
        p("+0x1.8p1", true);
        p("-0x1p1", false);
        p("+0x1p1", false);
        // suffixes
        p("0x1.8p1f", true);
        p("0x1.8p1F", true);
        p("0x1.8p1d", true);
        p("0x1.8p1D", true);
        // exponent signs and zeros
        p("0x1p-1", true);
        p("0x1p+1", true);
        p("0x1p-1", false);
        p("0x1p+1", false);
        p("0x1p01", true);
        p("0x1.8p+0", false);
        p("0x0p0", true);
        p("-0x0p0", true);
        p("+0x0p0", true);
        p("-0x0.0p0", false);
        p("0x0p-149", true);
        p("-0x0p-1074", false);
        // no 0x prefix forms
        p(".8p1", true);
        p(".8p1", false);
        p("8p1", true);
        p("1p1", true);
        // missing pieces
        p("0x1.8", true);
        p("0x1.8", false);
        p("0x1p", true);
        p("0xp1", true);
        p("0x", true);
        p("0x.8", false);
        p("0x1.8p", false);
        p("0x1.8p1.5", true);
        p("0x1.8px1", true);
        p("0x1.8p 1", true);
        p(" 0x1.8p1 ", true);
        p("0x1.8p1 ", false);
        // digits beyond precision (rounding)
        p("0x1.00000000000001p0", false);  // 1 + 2^-52 exact
        p("0x1.00000000000008p0", false);  // halfway tie -> even -> 1.0
        p("0x1.00000000000009p0", false);  // above halfway -> up
        p("0x1.0000000000000fp0", false);
        p("0x1.000001p0", true);           // f32: 1 + 2^-24 -> up
        p("0x1.0000008p0", true);          // f32 halfway tie -> 1.0
        p("0x1.0000009p0", true);          // f32 above -> up
        p("0x1.ffffffp-1", true);          // max-finite boundary check
        p("0x1.fffffep-1", true);
        // overflow
        p("0x1p128", true);
        p("0x1p127", true);
        p("0x1.fffffep127", true);
        p("0x1.ffffffp127", true);
        p("-0x1p128", true);
        p("0x1p1024", false);
        p("0x1p1023", false);
        p("0x1.fffffffffffffp1023", false);
        p("0x1.ffffffffffff8p1023", false);
        p("-0x1p2000", false);
        // subnormals
        p("0x1p-149", true);               // Float.MIN_VALUE
        p("0x1p-150", true);               // half min -> tie -> 0
        p("0x1.000001p-149", true);        // 1.5x min
        p("0x1.8p-150", true);             // 1.5x min (odd mantissa? tie behavior)
        p("0x1p-126", true);               // min normal
        p("0x1p-127", true);               // subnormal
        p("0x1p-1074", false);             // Double.MIN_VALUE
        p("0x1p-1075", false);             // half -> tie -> 0
        p("0x1.8p-1075", false);           // 1.5x -> rounds to 2 ulp? 
        p("0x1p-1022", false);             // min normal
        p("0x1p-1023", false);             // subnormal
        p("0x1.fffffffffffffp-1022", false); // just under min normal -> min normal
        p("-0x1p-1074", false);
        p("0x1p-3000", false);
        p("0x1p-3000", true);
        // long significands (trim + sticky)
        p("0x0.00000000000000000000000000001p0", false);
        p("0x1.0000000000000000000001p0", false);
        p("0x1.0000000000000000000001p0", true);
        p("0x123456789abcdefp0", false);
        p("0x123456789abcdefp0", true);
        p("0xfedcba987654321p-10", false);
        p("0x0000000000000000000001p3", true);
        // garbage hex
        p("0xg1p1", true);
        p("0x1.8.p1", true);
        p("0x1..8p1", true);
        p("0x1.8p1x", true);
        p("xx", true);
        p("0b101", true);
        p("0x1.8p1e2", true);
        p("0x1.8e2p1", true);
        p("0x0000p-5", false);
        // decimal forms unchanged (regression)
        p("2.5", true);
        p("1e10", false);
        p("Infinity", true);
        p("NaN", false);
        System.out.println("done " + n);
    }
}
