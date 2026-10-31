public class Shifts {
    public static void main(String[] args) {
        int a = -256;
        System.out.println(a << 1);
        System.out.println(a >> 1);
        System.out.println(a >>> 1);
        System.out.println(a >> 31);
        System.out.println(a >>> 28);
        System.out.println(a << 33);
        System.out.println(a >> 33);
        long b = -256L;
        System.out.println(b << 1);
        System.out.println(b >> 1);
        System.out.println(b >>> 1);
        System.out.println(b << 65);
        System.out.println(b >>> 66);
        for (int s = 0; s < 5; s++) {
            System.out.println((1 << s) + "," + (-16 >> s) + "," + (-16 >>> s));
        }
    }
}
