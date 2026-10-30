public class Boxing {
    public static void main(String[] args) {
        Integer a = Integer.valueOf(42);
        Integer b = 42;
        System.out.println(a.intValue());
        System.out.println(a + 1);
        System.out.println(a * b);
        Long l = 12345678900L;
        System.out.println(l + 1);
        System.out.println(l.longValue());
        Double d = 2.5;
        System.out.println(d / 5);
        Boolean flag = true;
        if (flag) System.out.println("boxed if");
        System.out.println("box=" + a + " long=" + l + " d=" + d + " flag=" + flag);
        Integer parsed = Integer.parseInt("-123");
        System.out.println(parsed);
        System.out.println(Integer.parseInt("+45"));
        try {
            Integer.parseInt("12x3");
            System.out.println("no error?");
        } catch (NumberFormatException e) {
            System.out.println("nfe");
        }
        try {
            Integer.parseInt("2147483648");
            System.out.println("no error?");
        } catch (NumberFormatException e) {
            System.out.println("overflow nfe");
        }
        System.out.println(Integer.toString(77));
        System.out.println(Long.parseLong("9000000000"));
        Object o = a;
        System.out.println(o instanceof Integer);
        System.out.println(o instanceof Long);
        Integer back = (Integer) o;
        System.out.println(back);
    }
}
