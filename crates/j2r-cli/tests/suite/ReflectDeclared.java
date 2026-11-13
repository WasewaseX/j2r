// R2 slice 3 differential fixture: the reflection ARRAY forms and the
// Constructor surface.
//   - Class.getDeclaredFields(): all declared fields, ANY modifier, no
//     inheritance, in DECLARATION order (Temurin prints source order).
//   - Class.getFields(): own publics in declaration order, then the
//     superclass chain's publics AFTER them, NO dedupe of shadowed names
//     (Child's pubBase AND Base's pubBase both appear). The interface-
//     constants leg is the registered residual (no interfaces table yet).
//   - Class.getDeclaredConstructors(): CLASSFILE order REVERSED (Temurin
//     lists the LAST-declared ctor first), Constructor.toString
//     ("protected C(double,boolean,char)" — dotted params, no space
//     after the comma), getModifiers (raw ACC_*), getName/getParameter
//     Types/getDeclaringClass.
//   - Class.getConstructors(): the public subset.
//   - getDeclaredConstructor/getConstructor: exact param match, the
//     Temurin miss message "Owner.<init>(float)".
// newInstance is the registered NEW-INSTANCE residual (needs the
// user-layout dispatch hook; JNI's NewObject is a stub today too).
// Oracle: Temurin 21.0.13+9, ReflectProbe capture (session 19b cont. 7).
public class ReflectDeclared {
    static class Base {
        public int pubBase = 1;
        private String privBase = "b";
        protected static long protStaticBase = 2L;
        public static final String PUB_SF = "sf";
    }
    static class Child extends Base {
        public double pubChild = 3.5;
        private char privChild = 'c';
        public int pubBase; // shadows the parent's public field
    }
    static class Ctorz {
        public Ctorz() { }
        Ctorz(int x) { }
        private Ctorz(String s, long l) { }
        protected Ctorz(double d, boolean b, char c) { }
        public int tag = 9;
    }

    public static void main(String[] args) throws Exception {
        Class<?> cc = Child.class;
        System.out.println("-- getDeclaredFields (Child) --");
        for (java.lang.reflect.Field f : cc.getDeclaredFields()) {
            System.out.println(f.getName() + " " + f.getType().getName()
                + " " + java.lang.reflect.Modifier.toString(f.getModifiers()));
        }
        System.out.println("-- getFields (Child) --");
        for (java.lang.reflect.Field f : cc.getFields()) {
            System.out.println(f.getName() + " " + f.getType().getName()
                + " " + f.getDeclaringClass().getName());
        }
        System.out.println("-- getFields (Base) --");
        for (java.lang.reflect.Field f : Base.class.getFields()) {
            System.out.println(f.getName() + " " + f.getDeclaringClass().getSimpleName());
        }
        System.out.println("-- getDeclaredConstructors (Ctorz) --");
        for (java.lang.reflect.Constructor<?> k : Ctorz.class.getDeclaredConstructors()) {
            System.out.println(k.toString());
            System.out.print("  params:");
            for (Class<?> p : k.getParameterTypes()) System.out.print(" " + p.getName());
            System.out.println();
            System.out.println("  name=" + k.getName() + " mods=" + k.getModifiers()
                + " decl=" + k.getDeclaringClass().getName());
        }
        System.out.println("-- getConstructors (Ctorz) --");
        for (java.lang.reflect.Constructor<?> k : Ctorz.class.getConstructors()) {
            System.out.println(k.toString());
        }
        System.out.println("-- lookups --");
        java.lang.reflect.Constructor<?> k = Ctorz.class.getDeclaredConstructor();
        System.out.println("no-arg=" + k.toString());
        System.out.println("int-form=" + Ctorz.class.getDeclaredConstructor(int.class).toString());
        System.out.println("two-form=" + Ctorz.class.getDeclaredConstructor(String.class, long.class).toString());
        System.out.println("pub-only=" + Ctorz.class.getConstructor().toString());
        try {
            Ctorz.class.getDeclaredConstructor(float.class);
        } catch (NoSuchMethodException e) {
            System.out.println("nsme: " + e.getMessage());
        }
        try {
            Ctorz.class.getConstructor(int.class);
        } catch (NoSuchMethodException e) {
            System.out.println("nsme-pub: " + e.getMessage());
        }
        System.out.println("ReflectDeclared done");
    }
}
