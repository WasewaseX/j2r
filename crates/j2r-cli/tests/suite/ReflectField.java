import java.lang.reflect.Field;

public class ReflectField {
    public int pubInt = 11;
    public long pubLong = 22L;
    public double pubDouble = 3.5;
    public boolean pubBool = true;
    public String pubRef = "hello";
    static int statInt = 33;
    static String statRef = "static";

    static class Inner {
        private int hidden = 8;
    }

    public static void main(String[] args) throws Exception {
        Class<?> c = ReflectField.class;
        // Declared lookups.
        Field f = c.getDeclaredField("pubInt");
        System.out.println(f.getName());
        ReflectField o = new ReflectField();
        // Primitive boxing round-trip; identity with valueOf cache holds.
        Object boxed = f.get(o);
        System.out.println(boxed instanceof Integer);
        System.out.println(boxed.equals(Integer.valueOf(11)));
        System.out.println(boxed == Integer.valueOf(11));
        // Other primitives box to their exact wrappers.
        System.out.println(c.getDeclaredField("pubLong").get(o) instanceof Long);
        System.out.println(c.getDeclaredField("pubDouble").get(o) instanceof Double);
        System.out.println(c.getDeclaredField("pubBool").get(o) instanceof Boolean);
        System.out.println(c.getDeclaredField("pubRef").get(o));
        // set mutates the real field.
        f.set(o, Integer.valueOf(99));
        System.out.println(o.pubInt);
        f.set(o, 7); // autoboxed through the same path
        System.out.println(o.pubInt);
        // Static fields: receiver ignored (null and non-null both fine).
        Field sf = c.getDeclaredField("statInt");
        System.out.println(sf.get(null));
        sf.set(null, Integer.valueOf(44));
        System.out.println(statInt);
        sf.set(o, 55);
        System.out.println(statInt);
        Field sr = c.getDeclaredField("statRef");
        System.out.println(sr.get(null));
        // Declared-only: a miss throws with Temurin's message (name only).
        try {
            c.getDeclaredField("nope");
            System.out.println("unreachable");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Receiver type errors carry Temurin's exact accessor wording.
        Field pubRef = c.getDeclaredField("pubRef");
        try {
            f.get("not a ReflectField");
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            pubRef.set("not a ReflectField", "x");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Value type errors: wrong box class and null-into-primitive.
        try {
            f.set(o, "str");
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            f.set(o, null);
        } catch (Throwable t) {
            System.out.println(t);
        }
        try {
            pubRef.set(o, Integer.valueOf(1));
        } catch (Throwable t) {
            System.out.println(t);
        }
        // Instance-field get with null receiver is a hot-spot-shaped NPE.
        try {
            f.get(null);
        } catch (Throwable t) {
            System.out.println(t);
        }
    }
}
