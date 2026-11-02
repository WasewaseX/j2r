public class StringBuilderExt {
    public static void main(String[] args) {
        StringBuilder sb = new StringBuilder("abcdef");
        System.out.println(sb.reverse());
        System.out.println(sb);
        sb.insert(0, "X");
        System.out.println(sb);
        sb.insert(3, 42);
        System.out.println(sb);
        sb.insert(sb.length(), '!');
        System.out.println(sb);
        System.out.println(sb.deleteCharAt(sb.length() - 1));
        System.out.println(sb.delete(0, 2));
        System.out.println(sb.replace(0, 2, "JJ"));
        System.out.println(sb);
        System.out.println(sb.charAt(2));
        System.out.println(sb.indexOf("42"));
        System.out.println(sb.capacity() >= sb.length());
        sb.setLength(4);
        System.out.println(sb);
        sb.setLength(6);
        System.out.println(sb.length());

        StringBuilder empty = new StringBuilder();
        System.out.println(empty.capacity());
        System.out.println(empty.length());

        // bounds (three-channel pinned)
        try { sb.deleteCharAt(99); } catch (StringIndexOutOfBoundsException e) { System.out.println("d1:" + e.getMessage()); }
        try { sb.charAt(99); } catch (StringIndexOutOfBoundsException e) { System.out.println("d2:" + e.getMessage()); }
        try { sb.insert(99, "x"); } catch (StringIndexOutOfBoundsException e) { System.out.println("d3:" + e.getMessage()); }
        try { sb.delete(3, 1); } catch (StringIndexOutOfBoundsException e) { System.out.println("d4:" + e.getMessage()); }
        try { sb.replace(9, 1, "x"); } catch (StringIndexOutOfBoundsException e) { System.out.println("d5:" + e.getMessage()); }
        try { new StringBuilder(-1); } catch (NegativeArraySizeException e) { System.out.println("d6:" + e.getMessage()); }

        // chaining still works with appended object payload
        StringBuilder chain = new StringBuilder();
        chain.append("a").append(1).append(true).append(2.5).append('c');
        System.out.println(chain);
        Object o = new Object();
        StringBuilder obj = new StringBuilder();
        obj.append(o);
        System.out.println(obj.toString().startsWith("java.lang.Object@"));
    }
}
