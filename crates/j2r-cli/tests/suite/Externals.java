import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.Random;

/**
 * EXTERNAL-B4..B12 (jbox2d port wave, 2026-11-13): every surface the
 * independent jbox2d port measured as missing, pinned differentially.
 *
 * Sections:
 *   B4  — NoSuchFieldError catch-type verification + hierarchy law
 *   B5  — StrictMath as its own class sharing the Math implementations
 *   B6  — the 8 boxed constructors (fresh boxes; `new` never caches)
 *   B7  — AssertionError's primitive/Object constructors
 *   B8  — the reverse IEEE bit-reinterpretation statics
 *   B9  — the fdlibm transcendental surface (sin/cos/tan/atan/atan2/hypot)
 *   B10 — java.util.Random's 48-bit LCG, bit-identical
 *   B11 — ArrayList.toArray() / toArray(T[]) two-branch contract
 *   B12 — Arrays.sort range forms + Class.desiredAssertionStatus
 */
public class Externals {

    // ---- B4: the jbox2d World$1 shape — a <clinit> that CATCHES
    // NoSuchFieldError (javac emits this for JLS 13.4.9 binary-compat
    // bridges; the verifier must resolve the catch type through the
    // LinkageError family).
    static int b4Bridge;
    static {
        boolean caught = false;
        try {
            // the field EXISTS at this compile: no error, the bridge is
            // for future binary compatibility — exactly like World$1
            b4Bridge = Externals.class.hashCode();
        } catch (NoSuchFieldError e) {
            caught = true;
        }
        System.out.println("B4 clinit bridge ran, caught=" + caught);
    }

    static String lcgBand( long seed, int n, double lo, double hi ) {
        // deterministic LCG inputs across [lo, hi)
        StringBuilder sb = new StringBuilder();
        long x = seed;
        for (int i = 0; i < n; i++) {
            x = x * 6364136223846793005L + 1442695040888963407L;
            double d = lo + (double)((x >>> 11) & ((1L << 52) - 1)) / (double)(1L << 52) * (hi - lo);
            if (i > 0) sb.append(' ');
            sb.append(d);
        }
        return sb.toString();
    }

    static void trans(String label, double[] xs, boolean withMath) {
        StringBuilder sb = new StringBuilder(label).append(':');
        for (double v : xs) {
            if (withMath) {
                sb.append(' ')
                  .append(Double.toString(Math.sin(v)))
                  .append('|').append(Double.toString(Math.cos(v)))
                  .append('|').append(Double.toString(Math.tan(v)))
                  .append('|').append(Double.toString(Math.atan(v)))
                  .append('|').append(Double.toString(Math.atan2(v, 3.5 - v)))
                  .append('|').append(Double.toString(Math.hypot(v, 1.25 * v - 0.5)));
            }
            sb.append(' ')
              .append(Double.toString(StrictMath.sin(v)))
              .append('|').append(Double.toString(StrictMath.cos(v)))
              .append('|').append(Double.toString(StrictMath.tan(v)))
              .append('|').append(Double.toString(StrictMath.atan(v)))
              .append('|').append(Double.toString(StrictMath.atan2(v, 3.5 - v)))
              .append('|').append(Double.toString(StrictMath.hypot(v, 1.25 * v - 0.5)));
        }
        System.out.println(sb);
    }

    public static void main(String[] args) {
        // ------------------------------------------------ B4
        System.out.println("== B4 ==");
        boolean icceMatchesNsfe = false;
        try {
            throw new IncompatibleClassChangeError("probe");
        } catch (NoSuchFieldError e) {
            icceMatchesNsfe = true;
        } catch (IncompatibleClassChangeError e) {
            System.out.println("B4 icce handler: " + e.getMessage());
        }
        System.out.println("B4 icce-in-nsfe-handler=" + icceMatchesNsfe);
        Object nsfe = new NoSuchFieldError("nsfe");
        System.out.println("B4 nsfe instanceof ICCE=" + (nsfe instanceof IncompatibleClassChangeError)
            + " instanceof LinkageError=" + (nsfe instanceof LinkageError)
            + " instanceof Error=" + (nsfe instanceof Error));
        try {
            throw (NoSuchFieldError) nsfe;
        } catch (NoSuchFieldError e) {
            System.out.println("B4 rethrow: " + e.getMessage());
        }

        // ------------------------------------------------ B5
        System.out.println("== B5 ==");
        System.out.println("B5 abs " + StrictMath.abs(-5) + " " + StrictMath.abs(-7L)
            + " " + StrictMath.abs(-2.5f) + " " + StrictMath.abs(-2.5d));
        System.out.println("B5 maxmin " + StrictMath.max(3, 9) + " " + StrictMath.min(-3L, -9L)
            + " " + StrictMath.max(-0.0f, 0.0f) + " " + StrictMath.min(-0.0d, 0.0d));
        System.out.println("B5 floordiv " + StrictMath.floorDiv(-7, 2) + " " + StrictMath.floorMod(-7, 2)
            + " " + StrictMath.floorDiv(-7L, 2L) + " " + StrictMath.floorMod(-7L, 2L));
        System.out.println("B5 exact " + StrictMath.addExact(3, 4) + " " + StrictMath.multiplyExact(6L, 7L)
            + " " + StrictMath.toIntExact(9L));
        try {
            StrictMath.addExact(Integer.MAX_VALUE, 1);
        } catch (ArithmeticException e) {
            System.out.println("B5 overflow: " + e.getMessage());
        }
        System.out.println("B5 round " + StrictMath.round(2.5d) + " " + StrictMath.round(-2.5f)
            + " " + StrictMath.sqrt(2.0) + " " + StrictMath.floor(-1.5) + " " + StrictMath.ceil(-1.5));

        // ------------------------------------------------ B6
        System.out.println("== B6 ==");
        Integer a127 = new Integer(127);
        Integer b127 = new Integer(127);
        System.out.println("B6 fresh== " + (a127 == b127) + " equals=" + a127.equals(b127)
            + " value=" + a127.intValue());
        Integer cached = Integer.valueOf(127);
        System.out.println("B6 cached==valueOf " + (a127 == cached));
        System.out.println("B6 payload " + new Integer(-4000) + " " + new Long(1L << 40)
            + " " + new Double(0.125) + " " + new Float(-2.5f) + " " + new Boolean(true)
            + " " + new Character('k') + " " + new Short((short) -300) + " " + new Byte((byte) -99));
        System.out.println("B6 strings " + new Integer(" -41 ".trim()) + " " + new Long("+900000")
            + " " + new Double("2.5e-3") + " " + new Float("1.5") + " " + new Boolean("TRUE")
            + " " + new Boolean("true") + " " + new Boolean("tRue1"));
        try {
            new Integer("12x");
        } catch (NumberFormatException e) {
            System.out.println("B6 nfe: " + e.getMessage());
        }
        try {
            new Short("70000");
        } catch (NumberFormatException e) {
            System.out.println("B6 short range: " + e.getMessage());
        }
        try {
            new Byte("200");
        } catch (NumberFormatException e) {
            System.out.println("B6 byte range: " + e.getMessage());
        }

        // ------------------------------------------------ B7
        System.out.println("== B7 ==");
        AssertionError[] aes = {
            new AssertionError(),
            new AssertionError(true),
            new AssertionError(false),
            new AssertionError('w'),
            new AssertionError(42),
            new AssertionError(4200000000000L),
            new AssertionError(0.5f),
            new AssertionError(6.149717374e-4d),
            new AssertionError("plain"),
            new AssertionError((Object) null),
            new AssertionError((Object) 7),
        };
        for (AssertionError ae : aes) {
            String m = ae.getMessage();
            System.out.println("B7 msg=" + (m == null ? "null" : m)
                + " ts=" + ae.toString());
        }
        try {
            throw new AssertionError("thrown");
        } catch (AssertionError e) {
            System.out.println("B7 caught: " + e.getMessage());
        }

        // ------------------------------------------------ B8
        System.out.println("== B8 ==");
        int nanBits = 0x7fc12345;
        float nan = Float.intBitsToFloat(nanBits);
        System.out.println("B8 nan raw=" + Integer.toHexString(Float.floatToRawIntBits(nan))
            + " canon=" + Integer.toHexString(Float.floatToIntBits(nan))
            + " isNaN=" + Float.isNaN(nan));
        System.out.println("B8 roundtrip=" + (Float.intBitsToFloat(Float.floatToIntBits(0.25f)) == 0.25f));
        long dbits = 0x400921fb54442d18L;
        double dpi = Double.longBitsToDouble(dbits);
        System.out.println("B8 pi=" + dpi + " bits=" + Long.toHexString(Double.doubleToRawLongBits(dpi)));
        System.out.println("B8 neg0 " + (1.0 / Double.longBitsToDouble(0x8000000000000000L) == Double.NEGATIVE_INFINITY)
            + " inf " + Double.longBitsToDouble(0x7ff0000000000000L));
        double dnan = Double.longBitsToDouble(0x7ff8deadbeef0000L);
        System.out.println("B8 dnan canon=" + Long.toHexString(Double.doubleToLongBits(dnan))
            + " raw=" + Long.toHexString(Double.doubleToRawLongBits(dnan)));

        // ------------------------------------------------ B9
        // Math pins the aligned special set (its HotSpot intrinsics carry
        // the measured ~6.5-in-100k 1-ulp residual vs fdlibm — O1, open;
        // 10.1 and -2.0944 are two such inputs and are pinned on the
        // StrictMath surface instead, which IS fdlibm bit-for-bit).
        // Inputs stay below 2^20*pi/2 (~3.3e6): beyond it the reduction
        // panics by design (O2, the loud named residual).
        System.out.println("== B9 ==");
        double[] band1 = { 0.0, -0.0, 0.7853981633974483, 1.0, 1.5707963267948966,
            3.141592653589793, 4.71238898038469, 6.283185307179586,
            0.6874999, 0.67439999, 0.6744 };
        trans("B9 math", band1, true);
        double[] band2 = { 10.1, -2.0944, 1.2345678901234, 0.0499, 12345.678,
            2.0943951 };
        trans("B9 strict", band2, false);
        // deterministic LCG band (bit-exact through the fdlibm kernels)
        for (int g = 0; g < 2; g++) {
            long x = 999331L + g;
            double[] vals = new double[64];
            for (int i = 0; i < 64; i++) {
                x = x * 6364136223846793005L + 1442695040888963407L;
                int e = (int)((x >>> 40) % 10) - 4;   // 1e-4 .. < 1e5 < 2^20*pi/2
                vals[i] = ((x >>> 11) % 100000) / 100000.0 * Math.pow(10, e);
            }
            trans("B9 lcg" + g, vals, false);
        }
        System.out.println("B9 inf " + Math.sin(Double.POSITIVE_INFINITY) + " nan " + Math.cos(Double.NaN));

        // ------------------------------------------------ B10
        System.out.println("== B10 ==");
        Random r = new Random(42);
        StringBuilder sb = new StringBuilder("B10 nextInt:");
        for (int i = 0; i < 8; i++) sb.append(' ').append(r.nextInt());
        System.out.println(sb);
        sb = new StringBuilder("B10 bound17:");
        for (int i = 0; i < 16; i++) sb.append(' ').append(r.nextInt(17));
        System.out.println(sb);
        sb = new StringBuilder("B10 bound64:");
        for (int i = 0; i < 16; i++) sb.append(' ').append(r.nextInt(64));
        System.out.println(sb);
        sb = new StringBuilder("B10 bound1:");
        for (int i = 0; i < 8; i++) sb.append(' ').append(r.nextInt(1));
        System.out.println(sb);
        try {
            r.nextInt(0);
        } catch (IllegalArgumentException e) {
            System.out.println("B10 bound0: " + e.getMessage());
        }
        sb = new StringBuilder("B10 long:");
        for (int i = 0; i < 6; i++) sb.append(' ').append(r.nextLong());
        System.out.println(sb);
        sb = new StringBuilder("B10 bool:");
        for (int i = 0; i < 20; i++) sb.append(r.nextBoolean() ? '1' : '0');
        System.out.println(sb);
        sb = new StringBuilder("B10 float:");
        for (int i = 0; i < 8; i++) sb.append(' ').append(r.nextFloat());
        System.out.println(sb);
        sb = new StringBuilder("B10 double:");
        for (int i = 0; i < 8; i++) sb.append(' ').append(r.nextDouble());
        System.out.println(sb);
        sb = new StringBuilder("B10 gauss:");
        for (int i = 0; i < 10; i++) sb.append(' ').append(r.nextGaussian());
        System.out.println(sb);
        byte[] buf = new byte[11];
        new Random(7).nextBytes(buf);
        sb = new StringBuilder("B10 bytes:");
        for (byte b : buf) sb.append(' ').append(b);
        System.out.println(sb);
        Random rs = new Random(99);
        rs.setSeed(42);
        System.out.println("B10 reseed==42stream " + (rs.nextInt() == new Random(42).nextInt()));
        sb = new StringBuilder("B10 gauss2:");
        for (int i = 0; i < 4; i++) sb.append(' ').append(rs.nextGaussian());
        System.out.println(sb);

        // ------------------------------------------------ B11
        System.out.println("== B11 ==");
        ArrayList<String> list = new ArrayList<>();
        list.add("alpha"); list.add("beta"); list.add("gamma");
        Object[] plain = list.toArray();
        System.out.println("B11 plain " + plain.length + " " + plain.getClass().getName()
            + " " + Arrays.toString(plain));
        String[] exact = new String[3];
        Object[] ret = list.toArray(exact);
        System.out.println("B11 exact " + (ret == (Object[]) exact) + " " + Arrays.toString(exact));
        String[] bigger = new String[5];
        bigger[4] = "leftover";
        Object[] ret2 = list.toArray(bigger);
        System.out.println("B11 bigger " + (ret2 == (Object[]) bigger) + " len=" + ret2.length
            + " [3]=" + ret2[3] + " [4]=" + ret2[4]);
        Object[] smaller = list.toArray(new String[1]);
        System.out.println("B11 smaller " + (smaller != (Object[]) bigger) + " "
            + smaller.getClass().getName() + " " + Arrays.toString(smaller));
        ArrayList<Integer> ints = new ArrayList<>();
        ints.add(3); ints.add(1);
        Object[] boxed = ints.toArray();
        System.out.println("B11 boxed " + boxed[0] + " " + boxed[1] + " " + boxed.getClass().getName());

        // ------------------------------------------------ B12
        System.out.println("== B12 ==");
        String[] words = { "pear", "apple", "fig", "apple", "date" };
        Arrays.sort(words, 1, 4);
        System.out.println("B12 range " + Arrays.toString(words));
        Arrays.sort(words, 0, 5, Comparator.reverseOrder());
        System.out.println("B12 cmp " + Arrays.toString(words));
        Arrays.sort(words, 2, 2);  // empty range: no-op
        System.out.println("B12 empty " + Arrays.toString(words));
        Integer[] nums = { 5, 3, 9, 1, 7 };
        Arrays.sort(nums, 0, 4, null);  // null comparator: natural ordering
        System.out.println("B12 nullcmp " + Arrays.toString(nums));
        try {
            Arrays.sort(nums, 3, 1);
        } catch (IllegalArgumentException e) {
            System.out.println("B12 iae: " + e.getMessage());
        }
        try {
            Arrays.sort(nums, -1, 4);
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println("B12 aioobe-from: " + e.getMessage());
        }
        try {
            Arrays.sort(nums, 0, 6);
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println("B12 aioobe-to: " + e.getMessage());
        }
        System.out.println("B12 assertions " + Externals.class.desiredAssertionStatus()
            + " " + String.class.desiredAssertionStatus());

        System.out.println("done");
    }
}
