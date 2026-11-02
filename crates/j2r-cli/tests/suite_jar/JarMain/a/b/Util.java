package a.b;

public class Util {
    public static final String BOOT = "util-boot";

    static {
        System.out.println("clinit-util:" + BOOT);
    }

    public static int scale(int x) {
        return x * 3 + Counter.next();
    }
}
