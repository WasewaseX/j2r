public class ClassIdentity {
    static class Nested {
        int marker = 1;
    }

    public static void main(String[] args) {
        // ldc interning: two literals of the same class are one object.
        System.out.println(ClassIdentity.class == ClassIdentity.class);
        // getClass() interning: ldc and getClass agree.
        Object o = new Nested();
        System.out.println(o.getClass() == Nested.class);
        System.out.println(o.getClass() == ClassIdentity.class);
        // Nested classes are distinct classes (cross-type compare via equals:
        // javac rejects == between unrelated Class<X> / Class<Y>).
        System.out.println(ClassIdentity.class.equals(Nested.class));
        // hashCode is stable per Class object (identity, never the value).
        Class<?> c = Nested.class;
        System.out.println(c.hashCode() == c.hashCode());
        System.out.println(c.equals(Nested.class));
        System.out.println(c.equals(ClassIdentity.class));
        // Builtin and primitive classes participate in the same interning.
        System.out.println(String.class == "x".getClass());
        System.out.println(int.class == int.class);
        System.out.println(int.class.equals(String.class));
        // Identity survives an explicit collection (interned Class objects
        // are permanent roots, M3_PLAN 8.1).
        Class<?> before = Nested.class;
        for (int i = 0; i < 1000; i++) {
            new Nested();
        }
        System.gc();
        System.out.println(Nested.class == before);
        System.out.println(before.getName());
    }
}
