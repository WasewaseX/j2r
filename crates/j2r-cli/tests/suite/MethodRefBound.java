import java.util.ArrayList;
import java.util.function.Function;
import java.util.function.Supplier;

public class MethodRefBound {
    private String tag = "id";

    String tagged(String s) {
        return tag + ":" + s;
    }

    public static void main(String[] args) {
        // bound to a String local, native virtual target
        String hello = "Hello";
        Supplier<String> up = hello::toUpperCase;
        System.out.println(up.get());

        // bound to a user object, compiled virtual target
        MethodRefBound o = new MethodRefBound();
        Function<String, String> tg = o::tagged;
        System.out.println(tg.apply("a"));

        // bound to a native collection
        ArrayList<String> list = new ArrayList<>();
        Supplier<Integer> size = list::size;
        list.add("z");
        System.out.println(size.get());

        // the bound receiver is captured once: later reassignment of the
        // local must not change the closure's receiver
        ArrayList<String> other = list;
        Supplier<Integer> size2 = list::size;
        list = new ArrayList<>();
        list.add("1");
        list.add("2");
        System.out.println(size2.get());
        System.out.println(other.size());
    }
}
