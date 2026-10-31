public class CompoundAssign {
    public static void main(String[] args) {
        byte b = 100;
        b += 28;
        System.out.println(b);
        short s = 30000;
        s *= 2;
        System.out.println(s);
        char c = 'A';
        c += 2;
        System.out.println(c);
        long l = 1;
        l <<= 40;
        System.out.println(l);
        int x = -17;
        x %= 5;
        System.out.println(x);
        x = -17;
        x /= 5;
        System.out.println(x);
        double d = 1;
        d /= 3;
        System.out.println(d);
        String str = "n = ";
        int n = 3;
        str += n;
        str += ';';
        str += true;
        System.out.println(str);
        int bits = 0xF0;
        bits &= 0x3C;
        bits |= 0x03;
        bits ^= 0xFF;
        System.out.println(bits);
    }
}
