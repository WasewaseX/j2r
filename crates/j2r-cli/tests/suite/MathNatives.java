public class MathNatives {
    public static void main(String[] args) {
        System.out.println(Math.abs(-42));
        System.out.println(Math.abs(42));
        System.out.println(Math.abs(Integer.MIN_VALUE));
        System.out.println(Math.abs(-9000000000L));
        System.out.println(Math.abs(-2.75));
        System.out.println(Math.abs(-1.5f));
        System.out.println(Math.max(3, 9));
        System.out.println(Math.min(3, 9));
        System.out.println(Math.max(-5, -2));
        System.out.println(Math.max(2.5, 2.75));
        System.out.println(Math.min(-0.0, 0.0));
        System.out.println(Math.max(1.5f, 1.25f));
        System.out.println(Math.max(10000000000L, 9999999999L));
        System.out.println(Math.min(Long.MAX_VALUE, 0));
        double nan = 0.0 / 0.0;
        double m = Math.max(nan, 1.0);
        System.out.println(m != m);
        System.out.println(String.valueOf(123));
        System.out.println(String.valueOf(1.5));
        System.out.println(String.valueOf('x'));
        System.out.println(String.valueOf(true));
        Object o = null;
        System.out.println(String.valueOf(o));
        System.out.println(System.nanoTime() != 0);
        System.out.println(System.currentTimeMillis() > 1500000000000L);
    }
}
