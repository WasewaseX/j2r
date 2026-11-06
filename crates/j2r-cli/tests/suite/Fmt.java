import java.io.UnsupportedEncodingException;

/**
 * Temurin vector for the Formatter engine (the "PrintStream.printf /
 * String.format" loud gap — its own wave). Pins the common subset: %s %S,
 * %d with width/flags, %x %X %o, %f %e with HALF_UP rounding, %n, %%,
 * argument indexing (%2$s, %<), and the exact error behaviors.
 */
public class Fmt {

    public static void main(String[] args) throws UnsupportedEncodingException {
        // 1. %s and %S with width/precision/left-justify
        System.out.println(String.format("[%s]", "abc"));
        System.out.println(String.format("[%8s]", "abc"));
        System.out.println(String.format("[%-8s]", "abc"));
        System.out.println(String.format("[%.2s]", "abcdef"));
        System.out.println(String.format("[%S]", "abc"));

        // 2. %d: width, zero-pad, plus, grouping, negative
        System.out.println(String.format("[%d]", 42));
        System.out.println(String.format("[%6d]", 42));
        System.out.println(String.format("[%06d]", 42));
        System.out.println(String.format("[%+d]", 42));
        System.out.println(String.format("[%+d]", -42));
        System.out.println(String.format("[%,d]", 1234567));
        System.out.println(String.format("[%,d]", -1234567));
        System.out.println(String.format("[%-6d]", 42));
        System.out.println(String.format("[%(d]", -42));

        // 3. %x %X %o: padding, hash
        System.out.println(String.format("[%x]", 255));
        System.out.println(String.format("[%X]", 255));
        System.out.println(String.format("[%#x]", 255));
        System.out.println(String.format("[%08x]", 0x1f));
        System.out.println(String.format("[%o]", 8));
        System.out.println(String.format("[%x]", -255));

        // 4. %f: default 6 decimals, HALF_UP rounding on exact decimals
        System.out.println(String.format("[%f]", 3.14));
        System.out.println(String.format("[%.2f]", 3.14159));
        System.out.println(String.format("[%.2f]", 0.125));
        System.out.println(String.format("[%.2f]", 2.675));
        System.out.println(String.format("[%.0f]", 2.5));
        System.out.println(String.format("[%.0f]", 3.5));
        System.out.println(String.format("[%.0f]", -2.5));
        System.out.println(String.format("[%12.3f]", Math.PI));
        System.out.println(String.format("[%.2f]", -0.005));
        System.out.println(String.format("[%f]", 1.0 / 0.0));
        System.out.println(String.format("[%.2f]", -1.0 / 0.0));
        System.out.println(String.format("[%.2f]", 0.0 / 0.0));

        // 5. %e
        System.out.println(String.format("[%e]", 12345.6789));
        System.out.println(String.format("[%.1e]", 12345.6789));

        // 6. %n and %%
        System.out.println(String.format("a%nb"));
        System.out.println(String.format("[%d%%]", 50));

        // 7. argument indexing and reuse
        System.out.println(String.format("[%2$s][%1$s]", "first", "second"));
        System.out.println(String.format("[%s][%<s]", "once"));

        // 8. %b %c %h
        System.out.println(String.format("[%b]", true));
        System.out.println(String.format("[%b]", (Object) null));
        System.out.println(String.format("[%c]", 'Q'));
        System.out.println(String.format("[%h]", "key"));

        // 9. errors: unknown conversion, missing arg
        try {
            String.format("%q", 1);
        } catch (java.util.UnknownFormatConversionException e) {
            System.out.println("UFCE: " + e.getMessage());
        }
        try {
            String.format("%d", "x");
        } catch (java.util.IllegalFormatConversionException e) {
            System.out.println("IFCE: " + e.getMessage());
        }
        try {
            String.format("[%d]", new Object[]{});
        } catch (java.util.MissingFormatArgumentException e) {
            System.out.println("MFAE: " + e.getMessage());
        }
    }
}
