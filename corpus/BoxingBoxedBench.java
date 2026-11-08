/**
 * BoxingBench BOXED side — the readiness §5 step-3 measurement (the
 * owner's opt_v1.1 evidence: their hand-port's HashMap<String,i32>
 * beat the JVM's boxed HashMap<String,Integer> 2.34x on the profile-
 * reuse workload). The registered question: does j2r's generated
 * Map<String,Double> still pay per-entry Value boxing (obj_get/obj_set
 * inside the map natives + the dispatch machinery per access) compared
 * with a primitive-specialized container, and what is the honest ratio
 * on BOTH channels (JVM and j2r)?
 *
 * This stem runs the aggregation through a HashMap<String, Double>;
 * BoxingPrimBench runs the IDENTICAL arithmetic through a double[]
 * indexed by the loop id. Same xorshift64* stream, same key order, same
 * addition order per key — the printed checksums are bit-identical
 * across BOTH stems and BOTH channels (a four-way differential: each
 * stem's output must equal the other's on each side).
 *
 * Deterministic output only (the corpus law): fixed seed, doubles
 * printed via string concatenation, the checksum accumulates in FIXED
 * key order (no HashMap iteration order anywhere on the checksum path).
 */
import java.util.HashMap;
import java.util.Map;

public class BoxingBoxedBench {

  static final int KEYS = 26;
  static final int ROUNDS = 100_000;

  // xorshift64* — the same-law precedent as JamaBench's driver PRNG
  // (seedTail byte-identical vs Temurin pins the shift/xor semantics).
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
    String[] keys = new String[KEYS];
    Map<String, Double> m = new HashMap<String, Double>();
    for (int i = 0; i < KEYS; i++) {
      keys[i] = "profile-key-" + i;
      m.put(keys[i], 0.5);
    }
    double wsum = 0.0;
    for (int r = 0; r < ROUNDS; r++) {
      for (int i = 0; i < KEYS; i++) {
        double w = rnd() * 0.001;
        wsum += w;
        // two boxed map ops per iteration: get unwraps the Double,
        // put rewraps it — the per-entry Value boxing the register asks
        // about (obj_get/obj_set inside the map natives on the j2r
        // side, scalar-replaceable-but-real boxes on the JVM side).
        m.put(keys[i], m.get(keys[i]) + w);
      }
    }
    // Fixed key order — the checksum path never walks the hash order.
    double chk = 0.0;
    for (int i = 0; i < KEYS; i++) {
      chk += m.get(keys[i]);
    }
    System.out.println("wsum=" + wsum);
    System.out.println("chk=" + chk);
  }
}
