public class Hello {
    public static void main(String[] args) {
        System.out.println("Hello, J2R!");
        System.out.println(2 + 2);
        int a = 10;
        int b = 3;
        System.out.println(a / b);
        System.out.println(a % b);
        long big = 123456789123L;
        System.out.println(big * 2);
        double d = 2.5;
        System.out.println(d * 4);
        String s = "x=" + (a * b) + "!";
        System.out.println(s);
        int f = 1;
        for (int i = 2; i <= 6; i++) {
            f = f * i;
        }
        System.out.println("6! = " + f);
    }
}
