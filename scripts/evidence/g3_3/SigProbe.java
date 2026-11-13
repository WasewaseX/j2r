/**
 * G3.3 slice 1 probe — the generic-signature pin, as javac actually emits
 * it. Compiled by the pinned Temurin javac 21.0.13; the classfile is then
 * parsed by j2r-codegen's signature layer (tests/signature_probe.rs,
 * env-gated: J2R_SIG_PROBE_CLASS).
 *
 * The probe deliberately carries every pin shape the rewrite slices care
 * about, plus the refusals:
 *   cache — a Map<String, Double> FIELD        -> Double (the pin)
 *   blend — a Map<String, Double> PARAM        -> Double (the pin)
 *   m     — a Map<String, Double> LOCAL (LVTT) -> Double (the pin; the
 *           BoxingBoxedBench hot-loop shape: m.put(k, m.get(k) + w))
 *   im    — a Map<String, Integer> LOCAL       -> Integer (cached pair)
 *   rm    — a RAW Map local                    -> no pin (erased)
 *   wm    — a Map<String, ? extends Number>    -> no pin (bounded
 *           wildcard; the registered conservative refusal)
 */
import java.util.HashMap;
import java.util.Map;

public class SigProbe {

  static Map<String, Double> cache = new HashMap<String, Double>();

  static double blend(Map<String, Double> m, String k) {
    return m.get(k);
  }

  public static void main(String[] args) {
    Map<String, Double> m = new HashMap<String, Double>();
    Map<String, Integer> im = new HashMap<String, Integer>();
    Map rm = new HashMap();
    // The wildcard map is read-only by javac's own capture law (a
    // `? extends Number` map accepts NO put of a Double) — which is
    // exactly why the rewrite slices must refuse its pin.
    Map<String, ? extends Number> wm = new HashMap<String, Double>();
    m.put("a", 0.5);
    im.put("a", 1);
    rm.put("a", "v");
    // The census shape: valueOf -> put, get -> checkcast -> doubleValue.
    for (int i = 0; i < 3; i++) {
      m.put("a", m.get("a") + 0.25);
    }
    System.out.println("" + blend(m, "a") + " " + im.get("a") + " " + rm.get("a")
        + " " + wm.get("a") + " " + cache.size());
  }
}
