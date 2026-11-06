/**
 * JLS 4.10.3 array covariance, pinned differentially (corpus wave 3, JAMA):
 * S <: T implies S[] <: T[], and every array type is a subtype of Object —
 * so a double[][] flows into Object[] parameters (the JAMA shape:
 * Vector.copyInto(Object[]) receiving double[][]), assignment sites, and
 * aastore through an Object[]-declared local onto a runtime [[D array.
 * The verifier used to reject the parameter form (expected Array(Ref),
 * found Array(RefPrimArrayDouble)); the runtime store-check semantics are
 * the JDK's own. All output deterministic.
 */
public class ArrayCovarianceProbe {

  static double sumFirstLast(Object[] rows) {
    double s = 0.0;
    for (int i = 0; i < rows.length; i++) {
      double[] r = (double[]) rows[i];
      s += r[0] + r[r.length - 1];
    }
    return s;
  }

  static void fillSink(double[][] src, Object[] out) {
    for (int i = 0; i < src.length; i++) {
      out[i] = src[i];
    }
  }

  public static void main(String[] args) {
    double[][] m = {{1.0, 2.0, 3.0}, {4.0, 5.0, 6.0}};
    // [[D passed where Object[] is declared — the verifier's covariant pop.
    System.out.println(sumFirstLast(m));
    // [[D assigned to an Object[] local (covariance assignment).
    Object[] sink = new double[2][];
    fillSink(m, sink);
    System.out.println(sumFirstLast(sink));
    sink[0] = new double[]{9.0};
    System.out.println(((double[]) sink[0])[0]);
    // Runtime store check through the covariant view: [I is not [D.
    try {
      sink[0] = new int[]{1, 2};
    } catch (ArrayStoreException e) {
      System.out.println("store: " + e.getMessage());
    }
    // instanceof through the covariance.
    Object[] o = m;
    System.out.println(o[0] instanceof double[]);
    // String into a runtime [[D via an Object[]-declared local.
    try {
      Object[] bad = new double[1][];
      bad[0] = "x";
    } catch (ArrayStoreException e) {
      System.out.println("store2: " + e.getMessage());
    }
    // null stores always succeed (F2's polymorphism, through covariance).
    Object[] nil = new double[1][];
    nil[0] = null;
    System.out.println(nil[0] == null);
    // clone() on [[D — shallow, identity differs (wave-2 array clone law).
    double[][] c = m.clone();
    c[0][0] = -1.0;
    System.out.println(m[0][0] + " " + c[0][0]);
  }
}
