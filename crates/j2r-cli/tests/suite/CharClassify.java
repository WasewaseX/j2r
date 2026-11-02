public class CharClassify {
    public static void main(String[] args) {
        char[] chars = {'0', '9', 'a', 'z', 'A', 'Z', '_', ' ', '\t', '\n', '\u00e9', '\u4e16', '\u0661', '\u0968', '\uff11', '\u00df', '\u00c9'};
        for (char c : chars) {
            System.out.println(
                (int) c + ":"
                + Character.isDigit(c)
                + Character.isLetter(c)
                + Character.isLetterOrDigit(c)
                + Character.isWhitespace(c)
                + Character.isUpperCase(c)
                + Character.isLowerCase(c)
                + Character.toUpperCase(c)
                + Character.toLowerCase(c)
                + Character.getNumericValue(c)
            );
        }
        System.out.println(Character.isDigit('\u0662'));
        System.out.println(Character.isDigit('\u00e9'));
        System.out.println(Character.isLetter('\u00e9'));
        System.out.println(Character.isWhitespace('\u00a0'));
        System.out.println(Character.isWhitespace('\u2028'));
        System.out.println(Character.isWhitespace('\u001c'));
        System.out.println(Character.toUpperCase('\u00e9'));
        System.out.println(Character.toLowerCase('\u00c9'));
        System.out.println(Character.getNumericValue('\u0664'));
        System.out.println(Character.getNumericValue('\uff19'));
        System.out.println(Character.getNumericValue('\u2160'));
        System.out.println(Character.getNumericValue('\u216c'));
    }
}
