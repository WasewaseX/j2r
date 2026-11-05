public class LambdaClinit {
    static final Runnable BOOT = () -> System.out.println("boot-ok");
    static volatile java.util.function.Supplier<String> MSG = () -> "from-lambda";
    public static void main(String[] args) {
        BOOT.run();
        System.out.println(MSG.get());
    }
}
