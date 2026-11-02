import java.util.ArrayList;
import java.util.function.Function;
import java.util.function.Supplier;

public class MethodRefCtor {
    static class Box {
        final int v;

        Box(int v) {
            this.v = v;
        }
    }

    public static void main(String[] args) {
        // constructor reference to a native class
        Supplier<ArrayList<String>> mk = ArrayList::new;
        ArrayList<String> a = mk.get();
        a.add("x");
        System.out.println(a);

        // constructor reference to a compiled class with an argument
        Function<Integer, Box> mkbox = Box::new;
        Box b = mkbox.apply(5);
        System.out.println(b.v);

        Supplier<Object> o = Object::new;
        System.out.println(o.get() instanceof Object);
    }
}
