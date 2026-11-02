import java.util.function.BiConsumer;
import java.util.function.BiFunction;
import java.util.function.BiPredicate;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;
import java.util.function.BinaryOperator;

public class LambdaCaptureless {
    public static void main(String[] args) {
        Function<Integer, Integer> doubleIt = x -> x * 2;
        System.out.println(doubleIt.apply(21));

        Supplier<String> greet = () -> "hi";
        System.out.println(greet.get());

        Predicate<Integer> even = x -> x % 2 == 0;
        System.out.println(even.test(10) + " " + even.test(7));

        BiFunction<Integer, Integer, Integer> add = (a, b) -> a + b;
        System.out.println(add.apply(2, 3));

        UnaryOperator<String> bang = s -> s + "!";
        System.out.println(bang.apply("go"));

        BinaryOperator<Integer> max = (a, b) -> (a >= b) ? a : b;
        System.out.println(max.apply(4, 9));

        Consumer<String> out = s -> System.out.println("got " + s);
        out.accept("x");

        BiConsumer<String, Integer> pair = (s, n) -> System.out.println(s + n);
        pair.accept("n=", 5);

        BiPredicate<String, Integer> len = (s, n) -> s.length() == n;
        System.out.println(len.test("abc", 3));

        Supplier<Integer> zero = () -> 0;
        System.out.println(zero.get());
    }
}
