import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

public class ReflectMethod {
    public static int statAdd(int a, int b) {
        return a + b;
    }

    public String concat(String s, int n) {
        String r = s;
        for (int i = 0; i < n; i++) {
            r += "!";
        }
        return r;
    }

    public void boom() {
        throw new IllegalStateException("boom-msg");
    }

    public static void main(String[] args) throws Exception {
        Class<?> c = ReflectMethod.class;
        ReflectMethod o = new ReflectMethod();
        // Static invoke: receiver ignored, return value boxed.
        Method stat = c.getDeclaredMethod("statAdd", int.class, int.class);
        Object r = stat.invoke(null, Integer.valueOf(2), Integer.valueOf(3));
        System.out.println(r instanceof Integer);
        System.out.println(r.equals(Integer.valueOf(5)));
        // Temurin ignores a non-null receiver on a static invoke.
        System.out.println(stat.invoke(o, Integer.valueOf(10), Integer.valueOf(20)));
        // Instance invoke with boxing on both sides.
        Method conc = c.getDeclaredMethod("concat", String.class, int.class);
        System.out.println(conc.invoke(o, "go", Integer.valueOf(2)));
        System.out.println(conc.invoke(o, null, Integer.valueOf(1)));
        // Method objects resolve to the exact (name, descriptor) pair.
        Method concAgain = c.getDeclaredMethod("concat", String.class, int.class);
        System.out.println(conc == concAgain);
        System.out.println(conc.equals(concAgain));
        // Misses print Temurin's `Owner.name(params)` message.
        try {
            c.getDeclaredMethod("nope");
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            c.getDeclaredMethod("concat", String.class, long.class);
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            c.getDeclaredMethod("concat", String.class);
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Wrong argument counts.
        try {
            conc.invoke(o, "x");
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            conc.invoke(o, "x", Integer.valueOf(1), Integer.valueOf(2));
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Wrong argument types.
        try {
            conc.invoke(o, "x", "not a number");
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            stat.invoke(null, Long.valueOf(1), Integer.valueOf(2));
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Null receiver on an instance method: hot-spot's accessor NPE.
        try {
            conc.invoke(null, "x", Integer.valueOf(1));
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Wrong receiver class.
        try {
            conc.invoke("not a ReflectMethod", "x", Integer.valueOf(1));
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Target exceptions are wrapped in InvocationTargetException with a
        // null message (Temurin prints the bare class name; getCause is
        // outside the M3 subset — documented in M3_EVIDENCE.md).
        Method bad = c.getDeclaredMethod("boom");
        try {
            bad.invoke(o);
            System.out.println("unreachable");
        } catch (InvocationTargetException ite) {
            System.out.println(ite.getClass().getName());
            System.out.println(ite.getMessage() == null);
        } catch (Throwable t) {
            System.out.println("wrong wrapper: " + t);
        }
    }
}
