package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: MatrixSolve holds the decomposition-backed solvers (solve/solveTranspose/inverse/det/rank/cond/trace) of the JAMA Matrix, split from the
 * single-file Matrix source per the owner's scanaislop complexity
 * order (file above 441 lines). The moved bodies are the upstream
 * bodies with the receiver made explicit where the file split
 * requires it (the receiver is named mx so the bare upstream field
 * reads A/m/n become mx.A/mx.m/mx.n) - behavior byte-identical,
 * pinned by the javac+java stdout proof and the j2r three-channel
 * gate. Hand-maintained after this generation step.
 */

final class MatrixSolve {

   /** Solve A*X = B
   @param B    right hand side
   @return     solution if A is square, least squares solution otherwise
   */

   static Matrix solve (Matrix mx, Matrix B) {
      return (mx.m == mx.n ? (new LUDecomposition(mx)).solve(B) :
                       (new QRDecomposition(mx)).solve(B));
   }

   /** Solve X*A = B, which is also A'*X' = B'
   @param B    right hand side
   @return     solution if A is square, least squares solution otherwise.
   */

   static Matrix solveTranspose (Matrix mx, Matrix B) {
      return mx.transpose().solve(B.transpose());
   }

   /** Matrix inverse or pseudoinverse
   @return     inverse(A) if A is square, pseudoinverse otherwise.
   */

   static Matrix inverse (Matrix mx) {
      return solve(mx, MatrixIo.identity(mx.m,mx.m));
   }

   /** Matrix determinant
   @return     determinant
   */

   static double det (Matrix mx) {
      return new LUDecomposition(mx).det();
   }

   /** Matrix rank
   @return     effective numerical rank, obtained from SVD.
   */

   static int rank (Matrix mx) {
      return new SingularValueDecomposition(mx).rank();
   }

   /** Matrix condition (2 norm)
   @return     ratio of largest to smallest singular value.
   */

   static double cond (Matrix mx) {
      return new SingularValueDecomposition(mx).cond();
   }

   /** Matrix trace.
   @return     sum of the diagonal elements.
   */

   static double trace (Matrix mx) {
      double t = 0;
      for (int i = 0; i < Math.min(mx.m,mx.n); i++) {
         t += mx.A[i][i];
      }
      return t;
   }

  private MatrixSolve() {}
}
