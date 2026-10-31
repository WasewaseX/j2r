public class FloatEdge {
    public static void main(String[] args) {
        System.out.println(0.1 + 0.2);
        System.out.println(1.0 / 3.0);
        double zero = 0.0;
        System.out.println(1.0 / zero);
        System.out.println(-1.0 / zero);
        System.out.println(0.0 / zero);
        System.out.println(-zero);
        System.out.println(1 / 0.0 == Double.POSITIVE_INFINITY);
        try {
            System.out.println(1 / 0);
        } catch (ArithmeticException e) {
            System.out.println("int-div-zero caught");
        }
        float f = 3.5f;
        System.out.println(f);
        System.out.println(f / 0);
        System.out.println((float) 0.1);
        System.out.println(1.5e300 * 1.5e300);
    }
}
