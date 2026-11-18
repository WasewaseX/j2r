package jamaentry;

/**
 * The norm family of the reshape: norm1/normInf/normF moved verbatim from
 * the vendored NIST Matrix.java as receiver-static helpers (scanaislop
 * round-4 wave — Matrix.java grew past the 441-line limit when the
 * JamaCorpus call graph added its delegators). norm2 stays on Matrix
 * (it delegates to the SVD). The bodies are byte-faithful with the mx
 * receiver prefixed on the A/m/n fields. Arithmetic and evaluation order
 * are unchanged (byte-identity gated by BOTH drivers).
 */
final class MatrixNorms {

  private MatrixNorms() {
    // not meant to be instantiated
  }

  /** One norm
  @return    maximum column sum.
  */
  static double norm1 (Matrix mx) {
     double f = 0;
     for (int j = 0; j < mx.n; j++) {
        double s = 0;
        for (int i = 0; i < mx.m; i++) {
           s += Math.abs(mx.A[i][j]);
        }
        f = Math.max(f,s);
     }
     return f;
  }

  /** Infinity norm
  @return    maximum row sum.
  */
  static double normInf (Matrix mx) {
     double f = 0;
     for (int i = 0; i < mx.m; i++) {
        double s = 0;
        for (int j = 0; j < mx.n; j++) {
           s += Math.abs(mx.A[i][j]);
        }
        f = Math.max(f,s);
     }
     return f;
  }

  /** Frobenius norm
  @return    sqrt of sum of squares of all elements.
  */
  static double normF (Matrix mx) {
     double f = 0;
     for (int i = 0; i < mx.m; i++) {
        for (int j = 0; j < mx.n; j++) {
           f = Maths.hypot(f,mx.A[i][j]);
        }
     }
     return f;
  }

}
