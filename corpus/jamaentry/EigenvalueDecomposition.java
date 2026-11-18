package jamaentry;


/** Eigenvalues and eigenvectors of a real matrix. 
<P>
    If A is symmetric, then A = V*D*V' where the eigenvalue matrix D is
    diagonal and the eigenvector matrix V is orthogonal.
    I.e. A = V.times(D.times(V.transpose())) and 
    V.times(V.transpose()) equals the identity matrix.
<P>
    If A is not symmetric, then the eigenvalue matrix D is block diagonal
    with the real eigenvalues in 1-by-1 blocks and any complex eigenvalues,
    lambda + i*mu, in 2-by-2 blocks, [lambda, mu; -mu, lambda].  The
    columns of V represent the eigenvectors in the sense that A*V = V*D,
    i.e. A.times(V) equals V.times(D).  The matrix V may be badly
    conditioned, or even singular, so the validity of the equation
    A = V*D*inverse(V) depends upon V.cond().
**/

public class EigenvalueDecomposition implements java.io.Serializable {

/* ------------------------
   Class variables
 * ------------------------ */

   /** Row and column dimension (square matrix).
   @serial matrix dimension.
   */
   int n;

   /** Symmetry flag.
   @serial internal symmetry flag.
   */
   private boolean issymmetric;

   /** Arrays for internal storage of eigenvalues.
   @serial internal storage of eigenvalues.
   */
   double[] d, e;

   /** Array for internal storage of eigenvectors.
   @serial internal storage of eigenvectors.
   */
   double[][] V;

   /** Array for internal storage of nonsymmetric Hessenberg form.
   @serial internal storage of nonsymmetric Hessenberg form.
   */
   double[][] H;

   /** Working storage for nonsymmetric algorithm.
   @serial working storage for nonsymmetric algorithm.
   */
   double[] ort;

   /** Check for symmetry, then construct the eigenvalue decomposition
       Structure to access D and V.
   @param Arg    Square matrix
   */

   public EigenvalueDecomposition (Matrix Arg) {
      double[][] A = Arg.getArray();
      n = Arg.getColumnDimension();
      V = new double[n][n];
      d = new double[n];
      e = new double[n];

      issymmetric = true;
      for (int j = 0; (j < n) & issymmetric; j++) {
         for (int i = 0; (i < n) & issymmetric; i++) {
            issymmetric = (A[i][j] == A[j][i]);
         }
      }

      if (issymmetric) {
         for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
               V[i][j] = A[i][j];
            }
         }
   
         // Tridiagonalize + diagonalize (the symmetric branch family).
         new EigenSymmetric(this).run();

      } else {
         H = new double[n][n];
         ort = new double[n];
         
         for (int j = 0; j < n; j++) {
            for (int i = 0; i < n; i++) {
               H[i][j] = A[i][j];
            }
         }
   
         // Reduce to Hessenberg form, then to real Schur form (the
         // nonsymmetric branch family).
         new EigenOrthes(this).run();
         new EigenHqr2(this).run();
      }
   }

/* ------------------------
   Public Methods
 * ------------------------ */

   /** Return the eigenvector matrix
   @return     V
   */

   public Matrix getV () {
      return new Matrix(V,n,n);
   }

   /** Return the real parts of the eigenvalues
   @return     real(diag(D))
   */

   public double[] getRealEigenvalues () {
      return d;
   }

   /** Return the imaginary parts of the eigenvalues
   @return     imag(diag(D))
   */

   public double[] getImagEigenvalues () {
      return e;
   }

   /** Return the block diagonal eigenvalue matrix
   @return     D
   */

   public Matrix getD () {
      Matrix X = new Matrix(n,n);
      double[][] D = X.getArray();
      for (int i = 0; i < n; i++) {
         for (int j = 0; j < n; j++) {
            D[i][j] = 0.0;
         }
         D[i][i] = d[i];
         if (e[i] > 0) {
            D[i][i+1] = e[i];
         } else if (e[i] < 0) {
            D[i][i-1] = e[i];
         }
      }
      return X;
   }
  private static final long serialVersionUID = 1;
}
