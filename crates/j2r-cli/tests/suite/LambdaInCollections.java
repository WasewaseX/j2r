import java.util.ArrayList;
import java.util.List;

public class LambdaInCollections {
    public static void main(String[] args) {
        List<String> names = new ArrayList<>();
        names.add("ann");
        names.add("bob");
        names.add("cara");

        List<String> out = new ArrayList<>();
        names.forEach(n -> out.add(n.toUpperCase()));
        System.out.println(out);

        List<Integer> nums = new ArrayList<>();
        for (int i = 0; i < 8; i++) {
            nums.add(i);
        }
        boolean removed = nums.removeIf(n -> n % 2 == 1);
        System.out.println(removed + " " + nums);

        List<Integer> big = new ArrayList<>();
        for (int i = 0; i < 5; i++) {
            big.add(i * 10);
        }
        big.forEach(n -> {
            if (n >= 20) {
                System.out.println("big " + n);
            }
        });

        // empty-list edge
        List<String> empty = new ArrayList<>();
        empty.forEach(n -> System.out.println("never " + n));
        boolean r2 = empty.removeIf(n -> true);
        System.out.println(r2);
    }
}
