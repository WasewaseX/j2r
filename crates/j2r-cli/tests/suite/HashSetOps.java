import java.util.HashSet;

public class HashSetOps {
    public static void main(String[] args) {
        HashSet<String> set = new HashSet<>();
        System.out.println(set.add("one"));
        System.out.println(set.add("two"));
        System.out.println(set.add("one"));
        System.out.println(set);
        System.out.println(set.size());
        System.out.println(set.isEmpty());
        System.out.println(set.contains("one"));
        System.out.println(set.contains("three"));
        System.out.println(set.remove("two"));
        System.out.println(set.remove("two"));
        System.out.println(set);

        HashSet<Integer> nums = new HashSet<>();
        for (int i = 0; i < 15; i++) nums.add(i * 7);
        System.out.println(nums);
        System.out.println(nums.size());
        System.out.println(nums.add(14));
        System.out.println(nums.iterator().hasNext());

        int sum = 0;
        for (Integer n : nums) sum += n;
        System.out.println(sum);

        HashSet<String> nulls = new HashSet<>();
        nulls.add(null);
        System.out.println(nulls);
        nulls.remove(null);
        System.out.println(nulls.size());
    }
}
