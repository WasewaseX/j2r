public class Uncaught {
    static void boom() {
        throw new IllegalStateException("uncaught-boom-42");
    }

    public static void main(String[] args) {
        System.out.println("before");
        boom();
        System.out.println("after");
    }
}
