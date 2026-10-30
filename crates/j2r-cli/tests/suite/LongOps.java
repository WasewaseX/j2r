public class LongOps {
    public static void main(String[] args) {
        long min = Long.MIN_VALUE;
        long max = Long.MAX_VALUE;
        System.out.println(min);
        System.out.println(max);
        System.out.println(min / -1);
        System.out.println(min % -1);
        System.out.println(max + 1);
        System.out.println(min - 1);
        System.out.println(min * -1);
        long v = 1L << 62;
        System.out.println(v);
        System.out.println(v / 3);
        System.out.println(v >>> 30);
        System.out.println(v >> 30);
        System.out.println(-v >>> 62);
        System.out.println((long) 123456 * 654321);
        int i = 2000000000;
        long wide = i;
        System.out.println(wide + wide);
        System.out.println((int) (wide + wide));
        System.out.println((long) (i + i));
        long unsigned = -1L >>> 1;
        System.out.println(unsigned);
        System.out.println(unsigned & 0xffffffffL);
        if (v > 1000000000000L) System.out.println("huge");
        long a = 123456789L, b = 987654321L;
        System.out.println(a * b / 1000L);
        System.out.println(a < b);
        long fromHex = 0xdeadbeefcafeL;
        System.out.println(fromHex);
        System.out.println((int) fromHex);
    }
}
