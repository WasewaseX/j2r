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

        // 10. RESIDUALS (FmtResidualProbe wave): %E %G %a/%A, the ',' flag
        // legality, null through the numeric conversions, the parse-time
        // error laws — every row Temurin-verified (21.0.13+9, 2026-11-05).
        // 10a. %E — uppercase scientific; the WHOLE piece uppercases
        System.out.println(String.format("A1: [%E]", 12345.6789));
        System.out.println(String.format("A2: [%.1E]", 12345.6789));
        System.out.println(String.format("A3: [%E]", 0.0));
        System.out.println(String.format("A4: [%E]", -2.5e-30));
        System.out.println(String.format("A5: [%E]", 1.0 / 0.0));
        System.out.println(String.format("A6: [%E]", 0.0 / 0.0));
        System.out.println(String.format("A7: [%12.2E]", Math.PI));
        System.out.println(String.format("A8: [%012.2E]", -Math.PI));
        System.out.println(String.format("A9: [%E]", 9.99999999e5));
        System.out.println(String.format("A10: [%E]", (float) 12345.6789));
        System.out.println(String.format("U9: [%E]", -0.0));
        System.out.println(String.format("Y2: [%e]", 0.001));
        System.out.println(String.format("Y3: [%e]", 0.999));
        System.out.println(String.format("Y5: [%.2e]", 0.006789));
        System.out.println(String.format("Y7: [%e]", -0.5));
        System.out.println(String.format("U10: [%.0e]", 12345.0));

        // 10b. %G — general; rounding re-decides the branch AFTER the carry
        System.out.println(String.format("B1: [%G]", 1234.5678));
        System.out.println(String.format("B2: [%G]", 1e7));
        System.out.println(String.format("B3: [%G]", 1e6));
        System.out.println(String.format("B4: [%G]", 999999.0));
        System.out.println(String.format("B5: [%G]", 1e-5));
        System.out.println(String.format("B6: [%G]", 1e-4));
        System.out.println(String.format("B7: [%.2G]", 123.456));
        System.out.println(String.format("B8: [%.2G]", 12.34));
        System.out.println(String.format("B9: [%G]", 0.0));
        System.out.println(String.format("B10: [%G]", 1.0 / 0.0));
        System.out.println(String.format("B11: [%G]", 0.0 / 0.0));
        System.out.println(String.format("B12: [%G]", -1234.5678));
        System.out.println(String.format("B13: [%10.2G]", 123.456));
        System.out.println(String.format("S5: [%g]", 0.001));
        System.out.println(String.format("S6: [%.2G]", 9.99));
        System.out.println(String.format("S7: [%.3G]", 999.99));
        System.out.println(String.format("S9: [%g]", -0.0));
        System.out.println(String.format("S10: [%.10G]", 1.0 / 3.0));
        System.out.println(String.format("U7: [%.0G]", 12345.0));
        System.out.println(String.format("U8: [%12.4G]", 123.456));
        System.out.println(String.format("U12: [%G]", 0.5));
        System.out.println(String.format("V4: [%012.4G]", 123.456));
        try {
            System.out.println(String.format("B14: [%#G]", 1.0));
        } catch (java.util.FormatFlagsConversionMismatchException e) {
            System.out.println("B14: FFCME: " + e.getMessage());
        }

        // 10c. %a / %A — hex float
        System.out.println(String.format("C1: [%a]", 1.0));
        System.out.println(String.format("C2: [%a]", 3.14));
        System.out.println(String.format("C3: [%a]", 0.0));
        System.out.println(String.format("C4: [%a]", -0.0));
        System.out.println(String.format("C5: [%a]", 2.0));
        System.out.println(String.format("C6: [%a]", 0.5));
        System.out.println(String.format("C7: [%a]", 4.9e-324));
        System.out.println(String.format("C8: [%a]", 1.0 / 0.0));
        System.out.println(String.format("C9: [%a]", 0.0 / 0.0));
        System.out.println(String.format("C10: [%A]", 3.14));
        System.out.println(String.format("C11: [%A]", 1.0 / 0.0));
        System.out.println(String.format("C12: [%a]", 2.5f));
        System.out.println(String.format("C13: [%.2a]", 3.14));
        System.out.println(String.format("C14: [%a]", 1024.0));
        System.out.println(String.format("C15: [%a]", -3.14));
        System.out.println(String.format("C16: [%20a]", 1.0));
        System.out.println(String.format("T1: [%a]", Math.pow(2, -1023)));
        System.out.println(String.format("T2: [%a]", Math.pow(2, -1022) * 1.5));
        System.out.println(String.format("T3: [%.3a]", Math.pow(2, -1023)));
        System.out.println(String.format("U2: [%.13a]", 1.0));
        System.out.println(String.format("U3: [%.14a]", 3.14));
        System.out.println(String.format("V1: [%.1a]", 1.96875));
        System.out.println(String.format("V2: [%.0a]", 3.14));
        System.out.println(String.format("V11: [%a]", 1.96875));
        try {
            System.out.println(String.format("U13a: [%a]", 42));
        } catch (java.util.IllegalFormatConversionException e) {
            System.out.println("U13a: IFCE: " + e.getMessage());
        }

        // 10d. the ',' flag is legal ONLY on d/f/g
        int[] dvals = {0x12345678, 255};
        char[] dconvs = {'x', 'X', 'o'};
        for (char cv : dconvs) {
            for (int v : dvals) {
                try {
                    System.out.println("D: [" + String.format("%," + cv, v) + "]");
                } catch (java.util.FormatFlagsConversionMismatchException e) {
                    System.out.println("D: FFCME: " + e.getMessage());
                }
            }
        }
        try { System.out.println("X2: [" + String.format("%,a", 1.5) + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("X2: FFCME: " + e.getMessage()); }
        try { System.out.println("X3: [" + String.format("%,s", "ab") + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("X3: FFCME: " + e.getMessage()); }
        try { System.out.println("X4: [" + String.format("%,c", 'q') + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("X4: FFCME: " + e.getMessage()); }
        try { System.out.println("X5: [" + String.format("%,b", true) + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("X5: FFCME: " + e.getMessage()); }
        try { System.out.println("X6: [" + String.format("%,h", "k") + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("X6: FFCME: " + e.getMessage()); }
        try { System.out.println("W9: [" + String.format("%,e", 1.5) + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("W9: FFCME: " + e.getMessage()); }
        System.out.println(String.format("X1: [%,g]", 1234.5));
        System.out.println(String.format("X8: [%,G]", 1234.5));
        System.out.println(String.format("F1: [%,f]", 1234567.891));
        System.out.println(String.format("F2: [%,.2f]", 1234567.5));
        System.out.println(String.format("F3: [%,f]", -1234567.891));
        System.out.println(String.format("X7: [%,(d]", -1234));

        // 10e. null through the numeric conversions — "null"/"NULL"
        Object nul = null;
        String[][] nullRows = {
            {"%f", "E1"}, {"%e", "E2"}, {"%g", "E3"}, {"%a", "E4"},
            {"%d", "E5"}, {"%x", "E6"}, {"%c", "E7"}, {"%E", "E8"},
            {"%G", "E9"}, {"%A", "E10"}, {"%S", "W10"}, {"%h", "U4"},
        };
        for (String[] row : nullRows) {
            System.out.println(row[1] + ": [" + String.format(row[0], nul) + "]");
        }

        // 10f. the uppercase-variant family is REAL (S B H C X E G A)
        System.out.println(String.format("W1: [%B]", true));
        System.out.println(String.format("W2: [%B]", (Object) null));
        System.out.println(String.format("W3: [%H]", "key"));
        System.out.println(String.format("W4: [%C]", 'q'));
        System.out.println(String.format("W6: [%#X]", 255));

        // 10g. parse-time laws: MFW, flag mismatches, the IFCE class-name law
        try { System.out.println("V3: [" + String.format("%-d", 5) + "]"); }
        catch (java.util.MissingFormatWidthException e) { System.out.println("V3: MFW: " + e.getMessage()); }
        try { System.out.println("U6: [" + String.format("%#d", 5) + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("U6: FFCME: " + e.getMessage()); }
        try { System.out.println("S4: [" + String.format("%,x", nul) + "]"); }
        catch (java.util.FormatFlagsConversionMismatchException e) { System.out.println("S4: FFCME: " + e.getMessage()); }
        try { System.out.println("U13b: [" + String.format("%f", 42) + "]"); }
        catch (java.util.IllegalFormatConversionException e) { System.out.println("U13b: IFCE: " + e.getMessage()); }
        try { System.out.println("U13c: [" + String.format("%e", 42L) + "]"); }
        catch (java.util.IllegalFormatConversionException e) { System.out.println("U13c: IFCE: " + e.getMessage()); }
        try { System.out.println("U13d: [" + String.format("%g", 42) + "]"); }
        catch (java.util.IllegalFormatConversionException e) { System.out.println("U13d: IFCE: " + e.getMessage()); }
        try { System.out.println("W8: [" + String.format("%E", 42) + "]"); }
        catch (java.util.IllegalFormatConversionException e) { System.out.println("W8: IFCE: " + e.getMessage()); }
        try { System.out.println("V8: [" + String.format("%f", "x") + "]"); }
        catch (java.util.IllegalFormatConversionException e) { System.out.println("V8: IFCE: " + e.getMessage()); }
        try { System.out.println("V9: [" + String.format("%d", 3.14) + "]"); }
        catch (java.util.IllegalFormatConversionException e) { System.out.println("V9: IFCE: " + e.getMessage()); }
        try { System.out.println("W5: [" + String.format("%Q", 1) + "]"); }
        catch (java.util.UnknownFormatConversionException e) { System.out.println("W5: UFCE: " + e.getMessage()); }
    }
}
