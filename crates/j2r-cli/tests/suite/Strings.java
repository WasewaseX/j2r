public class Strings {
    public static void main(String[] args) {
        String a = "hello";
        String b = "hello";
        System.out.println(a == b);
        System.out.println(a.equals(b));
        System.out.println(a.equals("world"));
        System.out.println(a.length());
        System.out.println(a.isEmpty());
        System.out.println("".isEmpty());
        System.out.println(a.charAt(1));
        System.out.println(a.charAt(0) + a.charAt(4));
        System.out.println(a.hashCode());
        System.out.println("".hashCode());
        String joined = "a" + 1 + "b" + 2L + "c" + 3.5 + "d" + 'e' + "f" + true + "g" + (short) 7;
        System.out.println(joined);
        StringBuilder sb = new StringBuilder();
        sb.append("count:");
        for (int i = 0; i < 5; i++) sb.append(' ').append(i);
        String s = sb.toString();
        System.out.println(s);
        System.out.println(sb.length());
        StringBuilder sb2 = new StringBuilder("start");
        sb2.append('-').append(1.25).append('-').append(false);
        System.out.println(sb2.toString());
        String nullStr = null;
        System.out.println(nullStr);
        System.out.println("val=" + nullStr);
        String long1 = "";
        for (int i = 0; i < 5; i++) long1 = long1 + i;
        System.out.println(long1);
        System.out.println("con" + "cat");
    }
}
