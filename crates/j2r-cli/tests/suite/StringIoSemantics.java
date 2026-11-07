/**
 * Differential fixture: the java.io string-reader/writer semantics wave.
 *
 * Pins the JDK laws that the runtime-io wave (2026-11-07) implemented after
 * the external org.json conversion report flagged the class:
 *
 *   1. StringReader.read() returns UTF-16 CODE UNITS — a supplementary-plane
 *      character arrives as its surrogate pair over two reads (0xD83C,
 *      0xDF0C), never as a scalar value. The cursor model is unit-precise,
 *      so mark/reset lands back on the exact unit.
 *   2. StringReader surface, Temurin-21-EMPIRICAL (the probe caught the
 *      Java-13-era CSR behavior before it shipped): skip(n) is clamped and
 *      a NEGATIVE n returns 0 WITHOUT throwing (the old
 *      IllegalArgumentException is gone from the modern JDK); ready();
 *      markSupported() = true; mark(readAheadLimit) still rejects negative
 *      limits ("Read-ahead limit < 0"); reset() before any mark() resets
 *      to the BEGINNING (mark initializes to 0 — "Mark invalid" is gone);
 *      close() is idempotent and every subsequent operation throws
 *      IOException "Stream closed" (even reset before any mark); bulk-read
 *      range errors carry the modern Objects.checkFromIndexRange message
 *      ("Range [off, off + len) out of bounds for length n") and are
 *      checked BEFORE the len==0 short-circuit.
 *   3. StringWriter.write(int) keeps only the 16 low-order bits — JDK's
 *      `(char) c` law: write(0x1F340) appends the single unit U+F340, never
 *      the astral scalar. (Lone surrogate units degrade to U+FFFD in the
 *      runtime's Rust-String payload — a documented limit, deliberately
 *      not pinned here.)
 *   4. StringWriter flush/close are no-ops; toString() reflects all
 *      appended content.
 *
 * Every row is deterministic and message-exact vs Temurin 21.0.13.
 */
public class StringIoSemantics {
    public static void main(String[] args) throws Exception {
        // ---- StringWriter: append, write(int) 16-bit law ----------------
        java.io.StringWriter sw = new java.io.StringWriter();
        sw.write("ab");
        sw.write('c');
        sw.write(0x1F340); // JDK: the single unit 0xF340
        sw.write("de", 0, 2);
        char[] arr = {'x', 'y', 'z'};
        sw.write(arr, 1, 2);
        sw.flush();
        sw.close();
        System.out.println("sw=" + sw.toString());
        System.out.println("sw_flush_close_noop=" + (sw.toString().length() == 8));

        // ---- StringReader: drain, EOF, idempotent -1 --------------------
        java.io.StringReader sr = new java.io.StringReader("abc");
        System.out.println("drain=" + sr.read() + "," + sr.read() + "," + sr.read()
                + "," + sr.read() + "," + sr.read());

        // ---- THE UTF-16 law: astral char = two unit reads ---------------
        java.io.StringReader a = new java.io.StringReader("\uD83C\uDF0C");
        System.out.println("astral_units=" + a.read() + "," + a.read() + "," + a.read());

        // ---- unit-precise mark/reset across a surrogate pair ------------
        java.io.StringReader m = new java.io.StringReader("\uD83C\uDF0Cz");
        int u1 = m.read(); // 0xD83C
        m.mark(10);
        int u2 = m.read(); // 0xDF0C
        m.reset();
        int u2again = m.read(); // 0xDF0C again
        int last = m.read(); // 'z'
        System.out.println("markreset=" + u1 + "," + u2 + "," + u2again + "," + last);
        System.out.println("marksupported=" + m.markSupported());

        // ---- bulk read: unit-precise fill, partial tail -----------------
        java.io.StringReader b = new java.io.StringReader("\uD83C\uDF0Cab");
        char[] cbuf = new char[3];
        int n1 = b.read(cbuf, 0, 3);
        int n2 = b.read(cbuf, 0, 3);
        int n3 = b.read(cbuf, 0, 3);
        System.out.println("bulk=" + n1 + "," + n2 + "," + n3);
        System.out.println("bulk_units=" + (int) cbuf[0] + "," + (int) cbuf[1] + ","
                + (int) cbuf[2]);
        System.out.println("bulk_zero=" + b.read(new char[2], 0, 0));

        // ---- skip: clamped, resumes, negative message -------------------
        java.io.StringReader s = new java.io.StringReader("abcdef");
        long moved = s.skip(2);
        int next = s.read();
        long clamped = s.skip(100);
        int eof = s.read();
        System.out.println("skip=" + moved + "," + next + "," + clamped + "," + eof);
        java.io.StringReader sn = new java.io.StringReader("abc");
        long neg = sn.skip(-3); // Temurin 21: returns 0, no throw
        System.out.println("skipneg=" + neg + "," + sn.read());

        // ---- ready / mark errors ---------------------------------------
        java.io.StringReader r = new java.io.StringReader("q");
        System.out.println("ready=" + r.ready());
        try {
            r.mark(-1);
            System.out.println("markneg=NO-THROW");
        } catch (IllegalArgumentException e) {
            System.out.println("markneg=" + e.getMessage());
        }
        int consumed = r.read(); // 'q', pos = 1
        r.reset(); // Temurin 21: no mark ever set -> resets to the BEGINNING
        System.out.println("resettostart=" + consumed + "," + r.read());
        r.mark(1);
        r.reset();
        System.out.println("readderafterreset=" + r.read());

        // ---- close: every op throws, close idempotent -------------------
        java.io.StringReader c = new java.io.StringReader("ab");
        c.close();
        c.close();
        try { c.read(); System.out.println("cr=NO-THROW"); }
        catch (java.io.IOException e) { System.out.println("cr=" + e.getMessage()); }
        try { c.read(new char[2], 0, 2); System.out.println("cb=NO-THROW"); }
        catch (java.io.IOException e) { System.out.println("cb=" + e.getMessage()); }
        try { c.skip(1); System.out.println("cs=NO-THROW"); }
        catch (java.io.IOException e) { System.out.println("cs=" + e.getMessage()); }
        try { c.ready(); System.out.println("cy=NO-THROW"); }
        catch (java.io.IOException e) { System.out.println("cy=" + e.getMessage()); }
        try { c.reset(); System.out.println("cx=NO-THROW"); }
        catch (java.io.IOException e) { System.out.println("cx=" + e.getMessage()); }
        try { c.mark(1); System.out.println("cm=NO-THROW"); }
        catch (java.io.IOException e) { System.out.println("cm=" + e.getMessage()); }
        System.out.println("cms_closed=" + c.markSupported()); // plain true, no open check

        // ---- bulk range: the Objects.checkFromIndexRange message -------
        java.io.StringReader g = new java.io.StringReader("ab");
        try {
            g.read(new char[2], 1, 2);
            System.out.println("ioobe=NO-THROW");
        } catch (IndexOutOfBoundsException e) {
            System.out.println("ioobe=" + e.getClass().getName() + "|" + e.getMessage());
        }
        try {
            g.read(new char[2], 0, -1);
            System.out.println("ioobe2=NO-THROW");
        } catch (IndexOutOfBoundsException e) {
            System.out.println("ioobe2=" + e.getClass().getName() + "|" + e.getMessage());
        }

        // ---- empty string: EOF from the start ---------------------------
        java.io.StringReader e0 = new java.io.StringReader("");
        System.out.println("empty=" + e0.read() + "," + e0.ready());
    }
}
