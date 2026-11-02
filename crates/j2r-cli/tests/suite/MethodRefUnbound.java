import java.util.function.BiFunction;
import java.util.function.Function;

public class MethodRefUnbound {
    public static void main(String[] args) {
        // unbound native target: the receiver arrives as the first SAM arg
        Function<String, String> up = String::toUpperCase;
        System.out.println(up.apply("abc") + up.apply("de"));

        BiFunction<String, Integer, String> chop = String::substring;
        System.out.println(chop.apply("hello", 2));

        Function<String, Integer> len = String::length;
        System.out.println(len.apply("four"));
    }
}
