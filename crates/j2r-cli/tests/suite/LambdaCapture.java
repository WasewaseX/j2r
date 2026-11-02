import java.util.ArrayList;
import java.util.List;
import java.util.function.Function;
import java.util.function.Supplier;

public class LambdaCapture {
    static class Box {
        int v;
    }

    public static void main(String[] args) {
        // JLS 15.27.2: the capture is of the *reference*, by value — the
        // object it points to is shared, so mutations through it are visible.
        Box box = new Box();
        box.v = 10;
        Supplier<Integer> get = () -> box.v;
        System.out.println(get.get());
        box.v = 99;
        System.out.println(get.get());

        String prefix = "v";
        Function<String, String> tag = s -> prefix + "=" + s;
        System.out.println(tag.apply("x") + " " + tag.apply("y"));

        // mixed primitive capture kinds
        int a = 1;
        long l = 2L;
        double d = 0.5;
        Supplier<String> mixed = () -> a + "/" + l + "/" + d;
        System.out.println(mixed.get());

        // each loop iteration captures its own value (per-iteration copies)
        List<Supplier<Integer>> fs = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            final int k = i * 100;
            fs.add(() -> k);
        }
        for (Supplier<Integer> s : fs) {
            System.out.println(s.get());
        }
    }
}
