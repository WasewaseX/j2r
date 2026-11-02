import java.util.function.Supplier;

public class LambdaThis {
    private int inst = 7;
    private static int stat = 3;

    private Supplier<Integer> reader() {
        // implicit `this` capture (instance-host lambda)
        return () -> inst + stat;
    }

    private Supplier<Integer> explicit() {
        return () -> this.inst * 10;
    }

    public static void main(String[] args) {
        LambdaThis o = new LambdaThis();
        System.out.println(o.reader().get());
        System.out.println(o.explicit().get());
        o.inst = 9;
        System.out.println(o.reader().get());
    }
}
