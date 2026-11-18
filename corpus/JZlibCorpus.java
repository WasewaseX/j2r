import java.io.*;
import jzlibentry.*;
/**
 * Differential driver over the JZlib library (real-world corpus, entry 2).
 * Deterministic output only; every println is stable across JVMs.
 * Core deflate/inflate through the public ZStream API (levels 0-9
 * sampled, Z_FINISH/Z_NO_FLUSH semantics, wrapper off), Adler32 and
 * CRC32 checksums over a fixed vector, and the library version string.
 */
public class JZlibCorpus {
  public static void main(String[] args) throws Exception {
    String text = "the quick brown fox jumps over the lazy dog; repeat, repeat, repeat; 0123456789";
    byte[] data = text.getBytes("UTF-8");
    phase1(data, text);
    phase2(data, text);
  }

  // phase 1: core deflate/inflate through the public ZStream API (levels 0-9
  // sampled, Z_FINISH/Z_NO_FLUSH semantics, wrapper off), Adler32 and CRC32
  // checksums over a fixed vector, and the library version string.
  private static void phase1(byte[] data, String text) throws Exception {
    for (int level = 0; level <= 9; level += 3) {
      ZStream z = new ZStream();
      z.deflateInit(level);
      z.next_in = data; z.next_in_index = 0; z.avail_in = data.length;
      byte[] comp = new byte[256];
      z.next_out = comp; z.next_out_index = 0; z.avail_out = comp.length;
      int dret = z.deflate(JZlib.Z_FINISH);
      int n = z.next_out_index;
      z.deflateEnd();
      Inflater inf = new Inflater();
      inf.inflateInit();
      inf.next_in = comp; inf.next_in_index = 0; inf.avail_in = n;
      byte[] back = new byte[256];
      inf.next_out = back; inf.next_out_index = 0; inf.avail_out = back.length;
      int ret;
      do { ret = inf.inflate(JZlib.Z_NO_FLUSH); } while (ret == JZlib.Z_OK && inf.avail_in > 0);
      int m = inf.next_out_index;
      inf.inflateEnd();
      String round = new String(back, 0, m, "UTF-8");
      System.out.println("level=" + level + " dret=" + dret + " comp=" + n + " iret=" + ret
          + " back=" + m + " ok=" + round.equals(text));
    }
    Adler32 a = new Adler32();
    a.update(data, 0, data.length);
    System.out.println("adler=" + Long.toHexString(a.getValue()));
    CRC32 c = new CRC32();
    c.update(data, 0, data.length);
    System.out.println("crc=" + Long.toHexString(c.getValue()));
    System.out.println("JZlib=" + JZlib.version());
  }

  // phase 2 (unblocked when ByteArrayInputStream landed, skip/charset
  // wave): the STREAM wrappers over the landed stream builtins —
  // DeflaterOutputStream/InflaterInputStream, ZOutputStream/ZInputStream,
  // GZIPOutputStream/GZIPInputStream. Compressed sizes are deterministic
  // (same JZlib source both sides), so they diff too.
  private static void phase2(byte[] data, String text) throws Exception {
    // DeflaterOutputStream -> BAOS; InflaterInputStream over BAIS
    ByteArrayOutputStream bout = new ByteArrayOutputStream();
    DeflaterOutputStream dos = new DeflaterOutputStream(bout);
    dos.write(data, 0, data.length);
    dos.finish();
    dos.close();
    byte[] dcomp = bout.toByteArray();
    InflaterInputStream iis = new InflaterInputStream(new ByteArrayInputStream(dcomp));
    byte[] dout = new byte[256];
    int dk = 0, dr;
    while ((dr = iis.read(dout, dk, dout.length - dk)) > 0) dk += dr;
    iis.close();
    System.out.println("wrap-deflate n=" + dcomp.length + " ok="
        + new String(dout, 0, dk, "UTF-8").equals(text));

    // ZOutputStream -> BAOS; ZInputStream over BAIS (its own skip uses the
    // decompressor, not the builtin default — exercised separately)
    ByteArrayOutputStream zbout = new ByteArrayOutputStream();
    ZOutputStream zout = new ZOutputStream(zbout, JZlib.Z_BEST_SPEED);
    zout.write(data, 0, data.length);
    zout.finish();
    zout.flush();
    zout.close();
    byte[] zcomp = zbout.toByteArray();
    ZInputStream zin = new ZInputStream(new ByteArrayInputStream(zcomp));
    byte[] zoutb = new byte[256];
    int zk = 0, zr;
    while ((zr = zin.read(zoutb, zk, zoutb.length - zk)) > 0) zk += zr;
    System.out.println("zstream n=" + zcomp.length + " ok="
        + new String(zoutb, 0, zk, "UTF-8").equals(text)
        + " totalIn=" + zin.getTotalIn() + " totalOut=" + zin.getTotalOut());
    zin.close();

    // GZIPOutputStream -> BAOS; GZIPInputStream over BAIS (header parse +
    // CRC32 trailer both go through JZlib's own code)
    ByteArrayOutputStream gbout = new ByteArrayOutputStream();
    GZIPOutputStream gos = new GZIPOutputStream(gbout);
    gos.write(data, 0, data.length);
    gos.finish();
    gos.close();
    byte[] gz = gbout.toByteArray();
    GZIPInputStream gis = new GZIPInputStream(new ByteArrayInputStream(gz));
    byte[] gout = new byte[256];
    int gk = 0, gr;
    while ((gr = gis.read(gout, gk, gout.length - gk)) > 0) gk += gr;
    gis.close();
    System.out.println("gzip n=" + gz.length + " ok="
        + new String(gout, 0, gk, "UTF-8").equals(text));

    // ZInputStream.skip — JZlib's own decompressor-driven skip (differs
    // from the builtin default; both sides run JZlib's code)
    ZInputStream zskip = new ZInputStream(new ByteArrayInputStream(zcomp));
    long skipped = zskip.skip(20);
    System.out.println("zskip=" + skipped + " next=" + zskip.read());
    zskip.close();
  }
}
