import java.lang.reflect.Field;
import java.lang.reflect.Method;

/**
 * R2 slice 2 (2026-11-13): the static-pool reflection surface —
 * Class.getField's public-member search law, Field/Method.getModifiers,
 * and the static Field.get/set paths through the statics pool.
 */
public class ReflectStatic {

    static int PKG_STATIC = 42;
    private static String PRIV_STATIC = "priv";
    public static double PUB_STATIC = 0.25;
    public int pubInstance = 11;

    public ReflectStatic() {}

    public static int statMethod(int x) { return x + 1; }
    private int privMethod() { return 3; }

    static class Child extends ReflectStatic {
        // shadows the inherited public field with a package-private one:
        // the JDK's getField0 quirk — the search CONTINUES upward and
        // returns the inherited public field (a non-public same-name
        // field does not block).
        int PUB_STATIC = 99;
    }

    public static void main(String[] args) throws Exception {
        Class<?> c = ReflectStatic.class;

        // ---- static Field.get: the three payload shapes + the boxed
        // return classes (int -> Integer, String -> String, double ->
        // Double)
        System.out.println(c.getDeclaredField("PKG_STATIC").get(null));
        System.out.println(c.getDeclaredField("PRIV_STATIC").get(null));
        System.out.println(c.getDeclaredField("PUB_STATIC").get(null));
        Object boxed = c.getDeclaredField("PUB_STATIC").get(null);
        System.out.println(boxed.getClass().getName());

        // ---- static Field.set then read back through the ordinary static
        Field f = c.getDeclaredField("PKG_STATIC");
        f.set(null, 77);
        System.out.println(PKG_STATIC);
        System.out.println(f.getInt(null));

        // ---- getModifiers: the exact ACC_* masks
        System.out.println(c.getDeclaredField("PKG_STATIC").getModifiers());   // static
        System.out.println(c.getDeclaredField("PRIV_STATIC").getModifiers());  // private static
        System.out.println(c.getDeclaredField("PUB_STATIC").getModifiers());   // public static
        System.out.println(c.getDeclaredField("pubInstance").getModifiers());  // public
        Method sm = c.getDeclaredMethod("statMethod", int.class);
        System.out.println(sm.getModifiers());                                  // public static
        Method pm = c.getDeclaredMethod("privMethod");
        System.out.println(pm.getModifiers());                                  // private
        System.out.println(c.getDeclaredField("pubInstance").getType().getName());

        // ---- Class.getField: the public-member search law
        System.out.println(c.getField("PUB_STATIC").get(null));
        System.out.println(c.getField("pubInstance").getInt(new ReflectStatic()));
        // inherited public field through the superclass chain
        System.out.println(Child.class.getField("PUB_STATIC").getDeclaringClass().getName());
        try {
            c.getField("PKG_STATIC");  // package-private: NOT a public member
        } catch (NoSuchFieldException e) {
            System.out.println("getField pkg: " + e.getMessage());
        }
        try {
            c.getField("nope");
        } catch (NoSuchFieldException e) {
            System.out.println("getField miss: " + e.getMessage());
        }
        try {
            c.getField(null);
        } catch (NullPointerException e) {
            System.out.println("getField null NPE, msg=" + e.getMessage());
        }

        System.out.println("done");
    }
}
