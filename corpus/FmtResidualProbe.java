import java.util.UnknownFormatConversionException;
import java.util.IllegalFormatConversionException;
import java.util.FormatFlagsConversionMismatchException;

/**
 * Temurin vector for the Formatter RESIDUALS (the registered loud gaps of
 * the Formatter wave, docs/CORPUS_WAVE.md): uppercase %E %G, the hex-float
 * %a/%A family (incl. subnormals and precision), the ',' flag legality per
 * conversion, null through the numeric conversions, the uppercase-variant
 * family (S B H C X E G A), and the parse-time error laws (MFW / FFCME /
 * the lowercased-conversion IFCE message). Every row was pinned on
 * Temurin 21.0.13+9 (2026-11-05) BEFORE implementation and is matched
 * byte-identically by the Fmt differential fixture.
 */
public class FmtResidualProbe {

    public static void main(String[] args) {
        Object nul = null;

        // A. %E — uppercase scientific; the WHOLE piece uppercases
        System.out.println("A1: [" + String.format("%E", 12345.6789) + "]");
        System.out.println("A2: [" + String.format("%.1E", 12345.6789) + "]");
        System.out.println("A3: [" + String.format("%E", 0.0) + "]");
        System.out.println("A4: [" + String.format("%E", -2.5e-30) + "]");
        System.out.println("A5: [" + String.format("%E", 1.0 / 0.0) + "]");
        System.out.println("A6: [" + String.format("%E", 0.0 / 0.0) + "]");
        System.out.println("A7: [" + String.format("%12.2E", Math.PI) + "]");
        System.out.println("A8: [" + String.format("%012.2E", -Math.PI) + "]");
        System.out.println("A9: [" + String.format("%E", 9.99999999e5) + "]");
        System.out.println("A10: [" + String.format("%E", (float) 12345.6789) + "]");
        System.out.println("U9: [" + String.format("%E", -0.0) + "]");

        // B. %G — general; the rounding carry re-decides the branch
        System.out.println("B1: [" + String.format("%G", 1234.5678) + "]");
        System.out.println("B2: [" + String.format("%G", 1e7) + "]");
        System.out.println("B3: [" + String.format("%G", 1e6) + "]");
        System.out.println("B4: [" + String.format("%G", 999999.0) + "]");
        System.out.println("B5: [" + String.format("%G", 1e-5) + "]");
        System.out.println("B6: [" + String.format("%G", 1e-4) + "]");
        System.out.println("B7: [" + String.format("%.2G", 123.456) + "]");
        System.out.println("B8: [" + String.format("%.2G", 12.34) + "]");
        System.out.println("B9: [" + String.format("%G", 0.0) + "]");
        System.out.println("B10: [" + String.format("%G", 1.0 / 0.0) + "]");
        System.out.println("B11: [" + String.format("%G", 0.0 / 0.0) + "]");
        System.out.println("B12: [" + String.format("%G", -1234.5678) + "]");
        System.out.println("B13: [" + String.format("%10.2G", 123.456) + "]");
        System.out.println("S5: [" + String.format("%g", 0.001) + "]");
        System.out.println("S6: [" + String.format("%.2G", 9.99) + "]");
        System.out.println("S7: [" + String.format("%.3G", 999.99) + "]");
        System.out.println("S9: [" + String.format("%g", -0.0) + "]");
        System.out.println("S10: [" + String.format("%.10G", 1.0 / 3.0) + "]");
        System.out.println("U7: [" + String.format("%.0G", 12345.0) + "]");
        System.out.println("U8: [" + String.format("%12.4G", 123.456) + "]");
        System.out.println("U12: [" + String.format("%G", 0.5) + "]");
        System.out.println("V4: [" + String.format("%012.4G", 123.456) + "]");
        try {
            System.out.println("B14: [" + String.format("%#G", 1.0) + "]");
        } catch (FormatFlagsConversionMismatchException e) {
            System.out.println("B14: FFCME: " + e.getMessage());
        }

        // C. %a / %A — hex float
        System.out.println("C1: [" + String.format("%a", 1.0) + "]");
        System.out.println("C2: [" + String.format("%a", 3.14) + "]");
        System.out.println("C3: [" + String.format("%a", 0.0) + "]");
        System.out.println("C4: [" + String.format("%a", -0.0) + "]");
        System.out.println("C5: [" + String.format("%a", 2.0) + "]");
        System.out.println("C6: [" + String.format("%a", 0.5) + "]");
        System.out.println("C7: [" + String.format("%a", 4.9e-324) + "]");
        System.out.println("C8: [" + String.format("%a", 1.0 / 0.0) + "]");
        System.out.println("C9: [" + String.format("%a", 0.0 / 0.0) + "]");
        System.out.println("C10: [" + String.format("%A", 3.14) + "]");
        System.out.println("C11: [" + String.format("%A", 1.0 / 0.0) + "]");
        System.out.println("C12: [" + String.format("%a", 2.5f) + "]");
        System.out.println("C13: [" + String.format("%.2a", 3.14) + "]");
        System.out.println("C14: [" + String.format("%a", 1024.0) + "]");
        System.out.println("C15: [" + String.format("%a", -3.14) + "]");
        System.out.println("C16: [" + String.format("%20a", 1.0) + "]");
        System.out.println("T1: [" + String.format("%a", Math.pow(2, -1023)) + "]");
        System.out.println("T2: [" + String.format("%a", Math.pow(2, -1022) * 1.5) + "]");
        System.out.println("T3: [" + String.format("%.3a", Math.pow(2, -1023)) + "]");
        System.out.println("U2: [" + String.format("%.13a", 1.0) + "]");
        System.out.println("U3: [" + String.format("%.14a", 3.14) + "]");
        System.out.println("V1: [" + String.format("%.1a", 1.96875) + "]");
        System.out.println("V2: [" + String.format("%.0a", 3.14) + "]");
        System.out.println("V11: [" + String.format("%a", 1.96875) + "]");
        System.out.println("W7: [" + String.format("%.2a", 0x1.FFFp0) + "]");
        try {
            System.out.println("U13a: [" + String.format("%a", 42) + "]");
        } catch (IllegalFormatConversionException e) {
            System.out.println("U13a: IFCE: " + e.getMessage());
        }

        // D. the ',' flag is legal ONLY on d/f/g
        int[] dvals = {0x12345678, 255};
        char[] dconvs = {'x', 'X', 'o'};
        for (char cv : dconvs) {
            for (int v : dvals) {
                try {
                    System.out.println("D: [" + String.format("%," + cv, v) + "]");
                } catch (FormatFlagsConversionMismatchException e) {
                    System.out.println("D: FFCME: " + e.getMessage());
                }
            }
        }
        try { System.out.println("X2: [" + String.format("%,a", 1.5) + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("X2: FFCME: " + e.getMessage()); }
        try { System.out.println("X3: [" + String.format("%,s", "ab") + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("X3: FFCME: " + e.getMessage()); }
        try { System.out.println("X4: [" + String.format("%,c", 'q') + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("X4: FFCME: " + e.getMessage()); }
        try { System.out.println("X5: [" + String.format("%,b", true) + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("X5: FFCME: " + e.getMessage()); }
        try { System.out.println("X6: [" + String.format("%,h", "k") + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("X6: FFCME: " + e.getMessage()); }
        try { System.out.println("W9: [" + String.format("%,e", 1.5) + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("W9: FFCME: " + e.getMessage()); }
        System.out.println("X1: [" + String.format("%,g", 1234.5) + "]");
        System.out.println("X8: [" + String.format("%,G", 1234.5) + "]");
        System.out.println("X7: [" + String.format("%,(d", -1234) + "]");

        // E. null through the numeric conversions — "null"/"NULL"
        String[][] nullRows = {
            {"%f", "E1"}, {"%e", "E2"}, {"%g", "E3"}, {"%a", "E4"},
            {"%d", "E5"}, {"%x", "E6"}, {"%c", "E7"}, {"%E", "E8"},
            {"%G", "E9"}, {"%A", "E10"}, {"%S", "W10"}, {"%h", "U4"},
        };
        for (String[] row : nullRows) {
            System.out.println(row[1] + ": [" + String.format(row[0], nul) + "]");
        }

        // F. %,f — grouping on the integer part only
        System.out.println("F1: [" + String.format("%,f", 1234567.891) + "]");
        System.out.println("F2: [" + String.format("%,.2f", 1234567.5) + "]");
        System.out.println("F3: [" + String.format("%,f", -1234567.891) + "]");

        // G. the uppercase-variant family is REAL (S B H C X E G A)
        System.out.println("W1: [" + String.format("%B", true) + "]");
        System.out.println("W2: [" + String.format("%B", nul) + "]");
        System.out.println("W3: [" + String.format("%H", "key") + "]");
        System.out.println("W4: [" + String.format("%C", 'q') + "]");
        System.out.println("W6: [" + String.format("%#X", 255) + "]");

        // H. the sub-1.0 %e rows (the leading-zeros exponent law)
        System.out.println("Y1: [" + String.format("%e", 0.5) + "]");
        System.out.println("Y2: [" + String.format("%e", 0.001) + "]");
        System.out.println("Y3: [" + String.format("%e", 0.999) + "]");
        System.out.println("Y4: [" + String.format("%E", 0.001) + "]");
        System.out.println("Y5: [" + String.format("%.2e", 0.006789) + "]");
        System.out.println("Y6: [" + String.format("%e", 0.1) + "]");
        System.out.println("Y7: [" + String.format("%e", -0.5) + "]");
        System.out.println("U10: [" + String.format("%.0e", 12345.0) + "]");
        System.out.println("U14: [" + String.format("%e", 9.999e-5) + "]");

        // I. parse-time laws: MFW, flag mismatches, the IFCE class law
        try { System.out.println("S4: [" + String.format("%,x", nul) + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("S4: FFCME: " + e.getMessage()); }
        try { System.out.println("U5: [" + String.format("%-,10x", 255) + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("U5: FFCME: " + e.getMessage()); }
        try { System.out.println("V3: [" + String.format("%-d", 5) + "]"); }
        catch (java.util.MissingFormatWidthException e) { System.out.println("V3: MFW: " + e.getMessage()); }
        try { System.out.println("U6: [" + String.format("%#d", 5) + "]"); }
        catch (FormatFlagsConversionMismatchException e) { System.out.println("U6: FFCME: " + e.getMessage()); }
        try { System.out.println("U13b: [" + String.format("%f", 42) + "]"); }
        catch (IllegalFormatConversionException e) { System.out.println("U13b: IFCE: " + e.getMessage()); }
        try { System.out.println("U13c: [" + String.format("%e", 42L) + "]"); }
        catch (IllegalFormatConversionException e) { System.out.println("U13c: IFCE: " + e.getMessage()); }
        try { System.out.println("U13d: [" + String.format("%g", 42) + "]"); }
        catch (IllegalFormatConversionException e) { System.out.println("U13d: IFCE: " + e.getMessage()); }
        try { System.out.println("W8: [" + String.format("%E", 42) + "]"); }
        catch (IllegalFormatConversionException e) { System.out.println("W8: IFCE: " + e.getMessage()); }
        try { System.out.println("V8: [" + String.format("%f", "x") + "]"); }
        catch (IllegalFormatConversionException e) { System.out.println("V8: IFCE: " + e.getMessage()); }
        try { System.out.println("V9: [" + String.format("%d", 3.14) + "]"); }
        catch (IllegalFormatConversionException e) { System.out.println("V9: IFCE: " + e.getMessage()); }
        try { System.out.println("W5: [" + String.format("%Q", 1) + "]"); }
        catch (UnknownFormatConversionException e) { System.out.println("W5: UFCE: " + e.getMessage()); }
        try { System.out.println("U15: [" + String.format("%a", nul) + "]"); }
        catch (Exception e) { System.out.println("U15: " + e.getClass().getSimpleName() + ": " + e.getMessage()); }
    }
}
