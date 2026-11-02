public class IntegerParse {
    public static void main(String[] args) {
        System.out.println(Integer.parseInt("42"));
        System.out.println(Integer.parseInt("-42"));
        System.out.println(Integer.parseInt("+7"));
        System.out.println(Integer.parseInt("2147483647"));
        System.out.println(Integer.parseInt("-2147483648"));
        System.out.println(Integer.parseInt("ff", 16));
        System.out.println(Integer.parseInt("777", 8));
        System.out.println(Integer.parseInt("1010", 2));
        System.out.println(Integer.parseInt("z", 36));
        System.out.println(Integer.toBinaryString(255));
        System.out.println(Integer.toBinaryString(-1));
        System.out.println(Integer.toHexString(-1));
        System.out.println(Integer.toOctalString(64));
        System.out.println(Integer.toHexString(0));
        System.out.println(Integer.compare(3, 7));
        System.out.println(Integer.compare(7, 3));
        System.out.println(Integer.compare(5, 5));
        System.out.println(Integer.max(3, 9));
        System.out.println(Integer.min(3, 9));
        System.out.println(Integer.sum(3, 9));
        System.out.println(Integer.bitCount(255));
        System.out.println(Integer.bitCount(-1));
        System.out.println(Integer.numberOfLeadingZeros(1));
        System.out.println(Integer.numberOfLeadingZeros(0));
        System.out.println(Integer.numberOfTrailingZeros(8));
        System.out.println(Integer.highestOneBit(100));
        System.out.println(Integer.highestOneBit(0));
        System.out.println(Integer.lowestOneBit(12));
        System.out.println(Integer.rotateLeft(1, 4));
        System.out.println(Integer.rotateRight(16, 4));
        System.out.println(Integer.reverse(1));
        System.out.println(Long.parseLong("9999999999"));
        System.out.println(Long.MAX_VALUE);
        System.out.println(Long.numberOfLeadingZeros(1L));
        System.out.println(Long.highestOneBit(1024L));
        System.out.println(Long.bitCount(-1L));
        System.out.println(Long.toHexString(-1L));
        System.out.println(Long.compare(1L, 2L));
        System.out.println(Long.sum(2L, 3L));
        try { Integer.parseInt("abc"); } catch (NumberFormatException e) { System.out.println("p1:" + e.getMessage()); }
        try { Integer.parseInt(""); } catch (NumberFormatException e) { System.out.println("p2:" + e.getMessage()); }
        try { Integer.parseInt("99999999999"); } catch (NumberFormatException e) { System.out.println("p3:" + e.getMessage()); }
        try { Integer.parseInt("2147483648"); } catch (NumberFormatException e) { System.out.println("p4:" + e.getMessage()); }
        try { Integer.parseInt("--5"); } catch (NumberFormatException e) { System.out.println("p5:" + e.getMessage()); }
        try { Integer.parseInt("8", 8); } catch (NumberFormatException e) { System.out.println("p6:" + e.getMessage()); }
        try { Integer.parseInt(null); } catch (NumberFormatException e) { System.out.println("p7:" + e.getMessage()); }
        try { Long.parseLong("abc"); } catch (NumberFormatException e) { System.out.println("p8:" + e.getMessage()); }
    }
}
