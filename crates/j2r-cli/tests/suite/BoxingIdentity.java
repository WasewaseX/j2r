public class BoxingIdentity {
    public static void main(String[] args) {
        Integer a = 127;
        Integer b = 127;
        System.out.println(a == b);
        Integer c = 128;
        Integer d = 128;
        System.out.println(c == d);
        System.out.println(c.equals(d));
        Integer big = 1000;
        int un = big;
        System.out.println(un);
        System.out.println(big + 1);
        System.out.println(big < 2000);
        System.out.println(big.equals(1000));
        Integer zero = 0;
        System.out.println(zero.equals(0));
        // Long vs Integer are never equal
        Integer ii = 1;
        Long ll = 1L;
        System.out.println(ii.equals(ll));
        System.out.println(ll.equals(1L));
        // unboxing null throws NPE
        Integer nil = null;
        try {
            int boom = nil;
            System.out.println(boom);
        } catch (NullPointerException e) {
            System.out.println("npe on unbox");
        }
        // ternary unboxing mix
        int picked = true ? big : 5;
        System.out.println(picked);
        Number asNum = big;
        System.out.println(asNum instanceof Integer);
    }
}
