/**
 * BoxingBench PRIMITIVE side — the specialization twin of
 * BoxingBoxedBench (readiness §5 step 3). The IDENTICAL arithmetic as
 * the boxed stem — same xorshift64* stream consumed in the same order,
 * same key/index order, same addition order per key — but the container
 * is a double[] indexed by the loop id instead of a
 * HashMap<String, Double>. The printed checksums are bit-identical to
 * the boxed stem on every channel (the four-way differential).
 */
public class BoxingPrimBench {

  static final int KEYS = 26;
  static final int ROUNDS = 100_000;

  static long xs = 0x243F6A8885A308D3L;

  static long xorshift64star() {
    long x = xs;
    x ^= x << 13;
    x ^= x >>> 7;
    x ^= x << 17;
    xs = x;
    return x;
  }

  static double rnd() {
    return (double) ((xorshift64star() >>> 11) & 0xFFFFFL) / 1048576.0;
  }

  public static void main(String[] args) {
    double[] m = new double[KEYS];
    for (int i = 0; i < KEYS; i++) {
      m[i] = 0.5;
    }
    double wsum = 0.0;
    for (int r = 0; r < ROUNDS; r++) {
      for (int i = 0; i < KEYS; i++) {
        double w = rnd() * 0.001;
        wsum += w;
        // one array load + one array store, no box in sight — the
        // primitive-specialized path the register measures against.
        m[i] += w;
      }
    }
    double chk = 0.0;
    for (int i = 0; i < KEYS; i++) {
      chk += m[i];
    }
    System.out.println("wsum=" + wsum);
    System.out.println("chk=" + chk);
  }
}
