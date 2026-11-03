public class FPProbe {
    static void t(String name, Runnable r) {
        try {
            r.run();
        } catch (Throwable e) {
            System.out.println(name + ": " + e.getClass().getSimpleName() + " | " + e.getMessage());
        }
    }
    public static void main(String[] args) {
        System.out.println("plain: " + Float.parseFloat("2.5"));
        System.out.println("sci: " + Float.parseFloat("1e10"));
        System.out.println("suffixF: " + Float.parseFloat("1.5f"));
        System.out.println("suffixD: " + Float.parseFloat("1.5D"));
        System.out.println("inf: " + Float.parseFloat("Infinity"));
        System.out.println("neginf: " + Float.parseFloat("-Infinity"));
        System.out.println("nan: " + Float.parseFloat("NaN"));
        t("ws", () -> Float.parseFloat(" 1.5"));
        t("infLower", () -> Float.parseFloat("inf"));
        t("empty", () -> Float.parseFloat(""));
        t("junk", () -> Float.parseFloat("2.5x"));
        t("hex", () -> System.out.println("hex: " + Float.parseFloat("0x1.8p1")));
        t("nullArg", () -> Float.parseFloat(null));
        t("dots", () -> Float.parseFloat("2."));
        System.out.println("dots2: ok");
    }
}
