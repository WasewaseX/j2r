import java.util.function.Supplier;

public class LambdaGC {
    public static void main(String[] args) {
        // churn: many closures allocated and dropped; a few stay reachable.
        // Under forced GC (J2R_GC_EVERY_N=1) a collection happens at every
        // allocation, so any rooting bug in the capture path dies loudly.
        Supplier<Integer> keep = null;
        for (int i = 0; i < 20000; i++) {
            final int k = i;
            Supplier<Integer> s = () -> k;
            if (i % 5000 == 0) {
                keep = s;
            }
        }
        System.out.println(keep.get());

        int sum = 0;
        for (int i = 0; i < 100; i++) {
            final int k = i;
            Supplier<Integer> s = () -> k * 2;
            sum += s.get();
        }
        System.out.println(sum);
    }
}
