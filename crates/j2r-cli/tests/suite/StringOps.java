public class StringOps {
    public static void main(String[] args) {
        String s = "Hello";
        System.out.println(s.length());
        System.out.println(s.isEmpty());
        System.out.println("".isEmpty());
        System.out.println(s.charAt(1));
        System.out.println(s.charAt(s.length() - 1));
        System.out.println(s.equals("Hello"));
        System.out.println(s.equals("hellO"));
        String a = "jav";
        String b = "jav";
        System.out.println(a == b);
        String c = new StringBuilder("ja").append("v").toString();
        System.out.println(c == a);
        System.out.println(c.equals(a));
        System.out.println(String.valueOf(42) + String.valueOf(true) + String.valueOf('x'));
        System.out.println(String.valueOf(2.5));
        System.out.println(String.valueOf(-0.0));
        StringBuilder sb = new StringBuilder();
        sb.append("a").append(1).append(2L).append(3.5).append('c').append(true);
        sb.append((Object) null);
        System.out.println(sb.toString());
        System.out.println(sb.length());
        StringBuilder sb2 = new StringBuilder("seed");
        sb2.append('-');
        String built = sb2.toString();
        sb2.append('!');
        System.out.println(built);
        System.out.println(sb2.toString());
        String nil = null;
        System.out.println("x" + nil + "y");
        System.out.println('a' + 1);
        System.out.println("v=" + 'a' + 1);
        System.out.println("w=" + ('a' + 1));
    }
}
