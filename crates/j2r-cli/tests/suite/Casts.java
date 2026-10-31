public class Casts {
    public static void main(String[] args) {
        long l = 0x1_0000_00FFL;
        int i = (int) l;
        System.out.println(i);
        int neg = -300;
        byte b = (byte) neg;
        System.out.println(b);
        char c = (char) neg;
        System.out.println((int) c);
        short s = (short) 70000;
        System.out.println(s);
        System.out.println((int) 2147483647.9);
        System.out.println((int) -2147483648.9);
        System.out.println((int) 1e100);
        System.out.println((int) -1e100);
        System.out.println((long) 1e100);
        System.out.println((int) Double.NaN);
        System.out.println((long) Double.NEGATIVE_INFINITY);
        System.out.println((int) 3.99);
        System.out.println((int) -3.99);
        char x = 'A';
        int xi = x;
        char back = (char) (xi + 1);
        System.out.println(xi);
        System.out.println(back);
        float f = 0.1f;
        double d = f;
        System.out.println(d == 0.1f);
        System.out.println(d);
    }
}
