import java.util.ArrayList;
import java.util.Arrays;
import java.util.Random;

public class SurfProbe {
    static double physics(double x) {
        // the jbox2d-shaped surface: trans, boxed ctors, Random, catch
        // NoSuchFieldError in a clinit bridge, bit statics, toArray,
        // sort ranges
        return StrictMath.sin(x) * StrictMath.cos(x) + Math.hypot(x, -x);
    }
    static final int BRIDGE;
    static {
        int v = 1;
        try { v = SurfProbe.class.hashCode(); } catch (NoSuchFieldError e) { v = 2; }
        BRIDGE = v;
    }
    public static void main(String[] a) {
        Random r = new Random(42);
        double d = physics(r.nextDouble()) + new Integer(5) + Float.intBitsToFloat(0x3f800000);
        AssertionError ae = new AssertionError((Object) "x");
        ArrayList<String> l = new ArrayList<>();
        l.add("a");
        Object[] arr = l.toArray(new String[0]);
        Arrays.sort(arr, 0, 1, null);
        // genuinely missing from the surface: Math.asin (registered NOWHERE;
        // the fdlibm module carries sin/cos/tan/atan/atan2/hypot only)
        double never = Math.asin(0.5);
        System.out.println(d + " " + ae + " " + arr[0] + " " + BRIDGE);
        System.out.println(never);
    }
}
