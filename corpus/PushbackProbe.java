import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.PushbackInputStream;

/**
 * Temurin vector for PushbackInputStream — the last unmodelled stream
 * builtin. Pins: unread() single/byte[]/byte[]-range, read() and read([BII)
 * interplay with the pushback buffer, skip, unread-before-read, buffer
 * exhaustion ("Push back buffer is full"), markSupported (false), the
 * "Stream closed" family, available().
 */
public class PushbackProbe {

    static byte[] src() {
        try {
            return "abcdefghijklmnopqrstuvwxyz".getBytes("US-ASCII");
        } catch (java.io.UnsupportedEncodingException e) {
            throw new RuntimeException(e);
        }
    }

    public static void main(String[] args) throws Exception {
        // 1. unread(int) then read(): the pushed byte comes back first
        PushbackInputStream p = new PushbackInputStream(new ByteArrayInputStream(src()), 8);
        int c1 = p.read(); // 'a', moves forward
        p.unread(c1);      // push 'a' back
        System.out.println(c1 + " " + p.read() + " " + p.read());

        // 2. unread(byte[]) pushes a whole chunk (LIFO order on re-read)
        PushbackInputStream p2 = new PushbackInputStream(new ByteArrayInputStream(src()), 16);
        byte[] three = new byte[3];
        p2.read(three, 0, 3);
        p2.unread(three, 0, 3);
        byte[] again = new byte[3];
        p2.read(again, 0, 3);
        System.out.println(new String(again, "US-ASCII") + " " + p2.read());

        // 3. unread(byte[]) whole-array variant
        PushbackInputStream p3 = new PushbackInputStream(new ByteArrayInputStream(src()), 8);
        p3.read();
        p3.unread(new byte[]{120, 121}); // x y
        int x = p3.read();
        int y = p3.read();
        System.out.println((char) x + "" + (char) y + " " + (char) p3.read());

        // 4. pushback buffer exhaustion -> IOException "Push back buffer is full"
        PushbackInputStream p4 = new PushbackInputStream(new ByteArrayInputStream(src()), 2);
        p4.unread('1');
        p4.unread('2');
        try {
            p4.unread('3');
            System.out.println("overflow (wrong)");
        } catch (IOException e) {
            System.out.println("overflow: " + e.getMessage());
        }

        // 5. skip() interacts with the pushback region
        PushbackInputStream p5 = new PushbackInputStream(new ByteArrayInputStream(src()), 8);
        p5.unread('Z');
        System.out.println(p5.skip(2));
        System.out.println((char) p5.read());

        // 6. markSupported is FALSE; mark/reset throw IOException
        PushbackInputStream p6 = new PushbackInputStream(new ByteArrayInputStream(src()), 8);
        System.out.println(p6.markSupported());
        try {
            p6.reset();
        } catch (IOException e) {
            System.out.println("reset: " + e.getMessage());
        }

        // 7. close(): read/unread/available all throw "Stream closed"
        PushbackInputStream p7 = new PushbackInputStream(new ByteArrayInputStream(src()), 8);
        p7.close();
        try {
            p7.read();
        } catch (IOException e) {
            System.out.println("closed read: " + e.getMessage());
        }
        try {
            p7.unread('a');
        } catch (IOException e) {
            System.out.println("closed unread: " + e.getMessage());
        }
        try {
            p7.available();
        } catch (IOException e) {
            System.out.println("closed avail: " + e.getMessage());
        }

        // 8. available() = buffered pushback + inner available
        PushbackInputStream p8 = new PushbackInputStream(new ByteArrayInputStream(src()), 8);
        p8.unread('W');
        System.out.println(p8.available());
    }
}
