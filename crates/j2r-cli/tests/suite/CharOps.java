public class CharOps {
    public static void main(String[] args) {
        char a = 'A';
        char z = 'Z';
        System.out.println(z - a);
        char lower = (char) (a + 32);
        System.out.println(lower);
        System.out.println((int) lower);
        char digit = '7';
        int value = digit - '0';
        System.out.println(value);
        String s = "j2r-1.0";
        int letters = 0;
        int digits = 0;
        int others = 0;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z') {
                letters++;
            } else if (c >= '0' && c <= '9') {
                digits++;
            } else {
                others++;
            }
        }
        System.out.println(letters);
        System.out.println(digits);
        System.out.println(others);
        char c = 'm';
        switch (c) {
            case 'm': System.out.println("em"); break;
            case 'n': System.out.println("en"); break;
            default: System.out.println("other");
        }
        System.out.println('q' > 'p');
        System.out.println((char) (c + 1));
        char esc = '\n';
        System.out.println((int) esc);
        System.out.println('\t' == 9);
    }
}
