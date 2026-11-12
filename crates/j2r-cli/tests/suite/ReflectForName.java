
/** R2 slice 1 — Class.forName resolves runtime string lookups against the
 *  closed world (the generated crate's metadata registry + the builtin
 *  tables). Every line is Temurin-21 oracle-captured behavior (session 19):
 *  unknown name -> ClassNotFoundException whose message IS the input name;
 *  null name -> NPE "null"; "int" is not a class by name; the array
 *  descriptor form ([I, [Ljava.lang.String;) resolves; the 3-arg overload
 *  resolves the same. Deterministic output only (the corpus law). */
public class ReflectForName {
    public static String tag = "r2slice1";

    public static void main(String[] args) throws Exception {
        // compiled class in the closed world (this fixture's own registry row)
        Class<?> self = Class.forName("ReflectForName");
        System.out.println("self: " + self.getName());
        // name resolution only — Field.GET on a static is a separate
        // registered gap (the M3 subset's static-pool boundary)
        System.out.println("fieldName: " + self.getDeclaredField("tag").getName());

        // builtin names resolve through the builtin tables
        System.out.println("str: " + Class.forName("java.lang.String").getName());
        System.out.println("map: " + Class.forName("java.util.HashMap").getName());
        System.out.println("iface: " + Class.forName("java.io.Serializable").isInterface());
        System.out.println("root: " + Class.forName("java.lang.Object").getName());

        // array descriptor forms
        System.out.println("arr: " + Class.forName("[Ljava.lang.String;").getName());
        System.out.println("primArr: " + Class.forName("[I").getName());
        System.out.println("arrComp: " + Class.forName("[Ljava.lang.String;").getComponentType().getName());

        // the 3-arg overload: same closed world, loader ignored
        System.out.println(
            "three: "
                + Class.forName("ReflectForName", true, ClassLoader.getSystemClassLoader())
                    .getName());

        // unknown name -> ClassNotFoundException, message IS the name
        try {
            Class.forName("nope.Nope");
            System.out.println("unknown: unreachable");
        } catch (ClassNotFoundException e) {
            System.out.println("unknown: [" + e.getMessage() + "]");
        }

        // primitives are not classes by name
        try {
            Class.forName("int");
            System.out.println("prim: unreachable");
        } catch (ClassNotFoundException e) {
            System.out.println("prim: [" + e.getMessage() + "]");
        }

        // null name -> NPE with Temurin's exact message
        try {
            String nullName = null;
            Class.forName(nullName);
            System.out.println("null: unreachable");
        } catch (NullPointerException e) {
            System.out.println("null: [" + e.getMessage() + "]");
        }

        // 3-arg unknown -> CNFE too
        try {
            Class.forName("nope.Nope", false, ClassLoader.getSystemClassLoader());
            System.out.println("three-unknown: unreachable");
        } catch (ClassNotFoundException e) {
            System.out.println("three-unknown: [" + e.getMessage() + "]");
        }
    }
}
