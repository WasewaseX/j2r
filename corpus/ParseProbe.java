public class ParseProbe {
    static void t(String name, Runnable r) {
        try {
            r.run();
            System.out.println(name + ": ok");
        } catch (Throwable e) {
            System.out.println(name + ": " + e.getClass().getSimpleName() + " | " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        System.out.println("happy16: " + Long.parseLong("ff", 16));
        System.out.println("neg16: " + Long.parseLong("-ff", 16));
        System.out.println("minradix: " + Long.parseLong("101", 2));
        System.out.println("maxradix: " + Long.parseLong("zz", 36));
        t("radix1", () -> Long.parseLong("1", 1));
        t("radix37", () -> Long.parseLong("1", 37));
        t("baddigit16", () -> Long.parseLong("g", 16));
        t("baddigit10", () -> Long.parseLong("1x", 10));
        t("empty16", () -> Long.parseLong("", 16));
        t("null16", () -> Long.parseLong(null, 16));
        t("intmin16", () -> Integer.parseInt("80000000", 16));
        System.out.println("intminok: " + Integer.parseInt("-80000000", 16));
    }
}
