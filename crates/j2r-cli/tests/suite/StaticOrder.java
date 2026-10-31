public class StaticOrder {
    static class Config {
        static final int PORT = 8080;
        static final String NAME = "j2r";
        static int counter = init("counter", 0);
        static int other;
        static {
            System.out.println("Config static block, counter=" + counter);
            other = counter + 100;
        }
        static int init(String tag, int v) {
            System.out.println("init " + tag);
            return v + 1;
        }
    }
    static class UsesConfig {
        static int snapshot = Config.other;
        static {
            System.out.println("UsesConfig sees other=" + snapshot + " port=" + Config.PORT);
        }
    }
    public static void main(String[] args) {
        System.out.println("main starts");
        System.out.println(Config.PORT);
        System.out.println(Config.NAME);
        System.out.println(Config.other);
        new UsesConfig();
        System.out.println(UsesConfig.snapshot);
    }
}
