import java.io.ByteArrayInputStream;
import java.io.FilterInputStream;
import java.io.IOException;
import java.io.InputStream;

/**
 * Temurin vector for the InputStream.skip(J) default + ByteArrayInputStream /
 * FilterInputStream skip semantics (the "InputStream.skip(J) default" loud
 * gap from the JZlib wave).
 */
public class SkipDefault {

    /** A stream that ONLY overrides read() — everything else is the JDK default. */
    static class Tiny extends InputStream {
        private final byte[] data;
        private int pos;

        Tiny(byte[] data) { this.data = data; }

        @Override
        public int read() {
            return pos < data.length ? (data[pos++] & 0xff) : -1;
        }
    }

    public static void main(String[] args) throws Exception {
        byte[] src = "abcdefghij".getBytes("US-ASCII");

        // 1. ByteArrayInputStream.skip: positive, mid-stream, return value
        ByteArrayInputStream bis = new ByteArrayInputStream(src);
        System.out.println(bis.skip(3));
        System.out.println(bis.read());
        System.out.println(bis.skip(4));
        System.out.println(bis.read());

        // 2. skip beyond EOF returns the ACTUAL count
        ByteArrayInputStream bis2 = new ByteArrayInputStream(src);
        System.out.println(bis2.skip(100));

        // 3. skip(0) and negative: JDK contract — skip(0) is 0, negative
        //    returns 0 with no exception (ByteArrayInputStream)
        ByteArrayInputStream bis3 = new ByteArrayInputStream(src);
        System.out.println(bis3.skip(0));
        System.out.println(bis3.skip(-5));
        System.out.println(bis3.read());

        // 4. ByteArrayInputStream.close() is a NO-OP — use after close works
        //    (BufferedInputStream "Stream closed" state stays a registered
        //    loud gap; BIS is the phase-2 candidate this wave lands)
        ByteArrayInputStream bis4 = new ByteArrayInputStream(src);
        bis4.close();
        System.out.println(bis4.skip(2));
        System.out.println(bis4.read());

        // 5. FilterInputStream.skip delegates to the inner stream
        FilterInputStream fis = new FilterInputStream(new ByteArrayInputStream(src)) {};
        System.out.println(fis.skip(2));
        System.out.println(fis.read());

        // 6. THE GAP: InputStream.skip DEFAULT on a read()-only stream —
        //    the JDK loop-reads through a skip buffer; with n larger than
        //    the stream it drains to EOF and returns the actual count
        Tiny tiny = new Tiny(src);
        System.out.println(tiny.skip(4));
        System.out.println(tiny.read());
        Tiny tiny2 = new Tiny(src);
        System.out.println(tiny2.skip(100));
        System.out.println(tiny2.read());
        Tiny tiny3 = new Tiny(src);
        System.out.println(tiny3.skip(0));
        System.out.println(tiny3.skip(-5));
        System.out.println(tiny3.read());

        // 7. read(byte[], off, len) interplay: skip then bulk-read the rest
        Tiny tiny4 = new Tiny(src);
        System.out.println(tiny4.skip(1));
        byte[] buf = new byte[4];
        System.out.println(tiny4.read(buf, 0, 4));
        System.out.println(new String(buf, "US-ASCII"));
    }
}
