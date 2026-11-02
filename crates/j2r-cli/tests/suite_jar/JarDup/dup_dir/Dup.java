public class Dup {
    static {
        System.out.println("dup-clinit");
    }

    public static String tag() {
        return "from-dir";
    }
}
