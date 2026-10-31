public class IntOverflow {
    public static void main(String[] args) {
        int max = Integer.MAX_VALUE;
        int min = Integer.MIN_VALUE;
        System.out.println(max + 1);
        System.out.println(min - 1);
        System.out.println(max * 2);
        System.out.println(Math.abs(min));
        System.out.println(-min);
        try {
            System.out.println(min / -1);
        } catch (ArithmeticException e) {
            System.out.println("overflow-div caught");
        }
        System.out.println(-7 / 2);
        System.out.println(-7 % 2);
        System.out.println(7 / -2);
        System.out.println(7 % -2);
    }
}
