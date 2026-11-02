public class StringExt {
    public static void main(String[] args) {
        String s = "Hello, World";
        System.out.println(s.substring(7));
        System.out.println(s.substring(0, 5));
        System.out.println(s.indexOf("World"));
        System.out.println(s.indexOf("o", 5));
        System.out.println(s.indexOf('l'));
        System.out.println(s.lastIndexOf('l'));
        System.out.println(s.indexOf("zz"));
        System.out.println(s.concat("!!"));
        System.out.println(s.replace('l', 'L'));
        System.out.println(s.toUpperCase());
        System.out.println(s.toLowerCase());
        System.out.println("  pad  ".trim());
        System.out.println(s.compareTo("Hello"));
        System.out.println(s.compareTo("Hello, World"));
        System.out.println(s.compareToIgnoreCase("HELLO, WORLD"));
        System.out.println(s.startsWith("Hello"));
        System.out.println(s.endsWith("World"));
        System.out.println(s.contains(", W"));
        System.out.println(s.contains("zz"));
        System.out.println(s.equalsIgnoreCase("HELLO, world"));
        System.out.println(s.equalsIgnoreCase("nope"));
        char[] chars = "abc".toCharArray();
        System.out.println(chars.length + ":" + chars[0] + chars[1] + chars[2]);
        System.out.println("ab".repeat(3));
        System.out.println("".repeat(2) + "end");

        // substring bounds (three-channel pinned)
        try { s.substring(13); } catch (StringIndexOutOfBoundsException e) { System.out.println("s1:" + e.getMessage()); }
        try { s.substring(5, 2); } catch (StringIndexOutOfBoundsException e) { System.out.println("s2:" + e.getMessage()); }
        try { s.substring(-1); } catch (StringIndexOutOfBoundsException e) { System.out.println("s3:" + e.getMessage()); }
        try { s.charAt(20); } catch (StringIndexOutOfBoundsException e) { System.out.println("s4:" + e.getMessage()); }

        // repeat negative
        try { "x".repeat(-2); } catch (IllegalArgumentException e) { System.out.println("r1:" + e.getMessage()); }

        // Unicode extras (pinned differentially)
        String u = "h\u00e9llo \u4e16\u754c";
        System.out.println(u);
        System.out.println(u.toUpperCase());
        System.out.println("\u00df".toUpperCase());
        System.out.println("I\u0130stanbul".toLowerCase());
        System.out.println("\u4e16".repeat(2));
    }
}
