package a.b;

public class Counter {
    private static int count = 10;

    static {
        System.out.println("clinit-counter");
    }

    public static int next() {
        count += 2;
        return count;
    }
}
