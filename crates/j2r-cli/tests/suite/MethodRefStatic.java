import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.UnaryOperator;

public class MethodRefStatic {
    static int square(int x) {
        return x * x;
    }

    static String wrap(String s) {
        return "[" + s + "]";
    }

    public static void main(String[] args) {
        // compiled static target
        Function<Integer, Integer> sq = MethodRefStatic::square;
        System.out.println(sq.apply(7));

        // native static target
        Function<Integer, Integer> abs = Math::abs;
        System.out.println(abs.apply(-42));

        UnaryOperator<String> w = MethodRefStatic::wrap;
        System.out.println(w.apply("m"));

        // native static target with boxing
        Function<Integer, Integer> bv = Integer::valueOf;
        System.out.println(bv.apply(5));

        BiFunction<Integer, Integer, Integer> maxer = Math::max;
        System.out.println(maxer.apply(3, 9));
    }
}
