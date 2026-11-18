package jamaentry;


   /** Singular Value Decomposition.
   <P>
   For an m-by-n matrix A with m >= n, the singular value decomposition is
   an m-by-n orthogonal matrix U, an n-by-n diagonal matrix S, and
   an n-by-n orthogonal matrix V so that A = U*S*V'.
   <P>
   The singular values, sigma[k] = S[k][k], are ordered so that
   sigma[0] >= sigma[1] >= ... >= sigma[n-1].
   <P>
   The singular value decompostion always exists, so the constructor will
   never fail.  The matrix condition number and the effective numerical
   rank can be computed from this decomposition.
   */

public class SingularValueDecomposition implements java.io.Serializable {

/* ------------------------
   Class variables
 * ------------------------ */

   /** Arrays for internal storage of U and V.
   @serial internal storage of U.
   @serial internal storage of V.
   */
   double[][] U, V;

   /** Array for internal storage of singular values.
   @serial internal storage of singular values.
   */
   double[] s;

   /** Row and column dimensions.
   @serial row dimension.
   @serial column dimension.
   */
   int m, n;

/* ------------------------
   Constructor
 * ------------------------ */


   /** Construct the singular value decomposition
       Structure to access U, S and V.
   @param Arg    Rectangular matrix
   */

   public SingularValueDecomposition (Matrix Arg) {
      // Derived from LINPACK code.
      m = Arg.getRowDimension();
      n = Arg.getColumnDimension();
      int nu = Math.min(m,n);
      s = new double [Math.min(m+1,n)];
      U = new double [m][nu];
      V = new double [n][n];
      new SingularValueCore(this, Arg.getArrayCopy()).run();
   }

/* ------------------------
   Public Methods
 * ------------------------ */

   /** Return the left singular vectors
   @return     U
   */

   public Matrix getU () {
      return new Matrix(U,m,Math.min(m+1,n));
   }

   /** Return the right singular vectors
   @return     V
   */

   public Matrix getV () {
      return new Matrix(V,n,n);
   }

   /** Return the one-dimensional array of singular values
   @return     diagonal of S.
   */

   public double[] getSingularValues () {
      return s;
   }

   /** Return the diagonal matrix of singular values
   @return     S
   */

   public Matrix getS () {
      Matrix X = new Matrix(n,n);
      double[][] S = X.getArray();
      for (int i = 0; i < n; i++) {
         for (int j = 0; j < n; j++) {
            S[i][j] = 0.0;
         }
         S[i][i] = this.s[i];
      }
      return X;
   }

   /** Two norm
   @return     max(S)
   */

   public double norm2 () {
      return s[0];
   }

   /** Two norm condition number
   @return     max(S)/min(S)
   */

   public double cond () {
      return s[0]/s[Math.min(m,n)-1];
   }

   /** Effective numerical matrix rank
   @return     Number of nonnegligible singular values.
   */

   public int rank () {
      double eps = Math.pow(2.0,-52.0);
      double tol = Math.max(m,n)*s[0]*eps;
      int r = 0;
      for (int i = 0; i < s.length; i++) {
         if (s[i] > tol) {
            r++;
         }
      }
      return r;
   }
  private static final long serialVersionUID = 1;
}
