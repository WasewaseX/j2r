import java.io.BufferedInputStream;
import java.io.ByteArrayInputStream;
import java.io.IOException;

/**
 * Temurin vector for BufferedInputStream (stream wave 3).
 * Pins: read-through-buffer semantics, read([BII) across buffer boundaries,
 * skip (buffer hit, buffer miss with inner drain, negative, beyond EOF),
 * mark/reset with readlimit invalidation ("Resetting to invalid mark"),
 * the "Stream closed" use-after-close family, available(), markSupported.
 */
public class Buffered {

    static byte[] src(String s) {
        try {
            return s.getBytes("US-ASCII");
        } catch (java.io.UnsupportedEncodingException e) {
            throw new RuntimeException(e);
        }
    }

    public static void main(String[] args) throws Exception {
        byte[] data = src("abcdefghijklmnopqrstuvwxyz");

        // 1. read through the buffer; available(); markSupported
        BufferedInputStream b = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        System.out.println(b.read());
        System.out.println(b.available());
        System.out.println(b.markSupported());

        // 2. read([BII) spanning the buffer edge (buffer=8, read 5 then 6)
        byte[] out1 = new byte[11];
        int n1 = b.read(out1, 0, 5);
        int n2 = b.read(out1, 5, 6);
        System.out.println(n1 + " " + n2 + " " + new String(out1, 0, 11, "US-ASCII"));

        // 3. mark + reset within the readlimit; read() still buffered
        b.mark(4);
        int c1 = b.read();
        b.reset();
        int c2 = b.read();
        System.out.println("mark " + c1 + " reset " + c2);

        // 4. skip from the buffer (hits the buffered region)
        System.out.println(b.skip(3));
        System.out.println(b.read());

        // 5. skip larger than the remaining buffer: the inner drain path
        BufferedInputStream b2 = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        b2.read(); // pos=1
        System.out.println("skip-big " + b2.skip(20));
        System.out.println("after " + b2.read());

        // 6. negative skip -> 0, zero skip -> 0
        BufferedInputStream b3 = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        b3.read();
        System.out.println(b3.skip(-5));
        System.out.println(b3.skip(0));
        System.out.println(b3.read());

        // 7. skip beyond EOF -> actual count
        BufferedInputStream b4 = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        System.out.println("skip-eof " + b4.skip(100));
        System.out.println("eof-read " + b4.read());

        // 8. readlimit invalidation: mark(limit), then read MORE than limit
        //    (forces buffer growth) -> reset throws
        BufferedInputStream b5 = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        b5.mark(2);
        for (int i = 0; i < 10; i++) b5.read();
        try {
            b5.reset();
            System.out.println("reset ok (wrong)");
        } catch (IOException e) {
            System.out.println("invalid: " + e.getMessage());
        }

        // 9. mark with a large readlimit: buffer grows, reset still works
        BufferedInputStream b6 = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        b6.mark(50);
        for (int i = 0; i < 20; i++) b6.read();
        b6.reset();
        System.out.println("big-mark " + b6.read());

        // 10. close(): read/skip/available all throw "Stream closed"
        BufferedInputStream b7 = new BufferedInputStream(new ByteArrayInputStream(data), 8);
        b7.close();
        try {
            b7.read();
        } catch (IOException e) {
            System.out.println("closed read: " + e.getMessage());
        }
        try {
            b7.skip(1);
        } catch (IOException e) {
            System.out.println("closed skip: " + e.getMessage());
        }
        try {
            b7.available();
        } catch (IOException e) {
            System.out.println("closed avail: " + e.getMessage());
        }

        // 11. read() at EOF after a partial buffer read
        BufferedInputStream b8 = new BufferedInputStream(
            new ByteArrayInputStream(src("abc")), 8);
        System.out.println(b8.read() + " " + b8.read() + " " + b8.read() + " " + b8.read());
    }
}
