package jamaentry;


import java.text.NumberFormat;
import java.text.DecimalFormat;
import java.text.DecimalFormatSymbols;
import java.util.Locale;
import java.text.FieldPosition;
import java.io.PrintWriter;
import java.io.BufferedReader;
import java.io.StreamTokenizer;

/**
   Jama = Java Matrix class.
<P>
   The Java Matrix Class provides the fundamental operations of numerical
   linear algebra.  Various constructors create Matrices from two dimensional
   arrays of double precision floating point numbers.  Various "gets" and
   "sets" provide access to submatrices and matrix elements.  Several methods 
   implement basic matrix arithmetic, including matrix addition and
   multiplication, matrix norms, and element-by-element array operations.
   Methods for reading and printing matrices are also included.  All the
   operations in this version of the Matrix Class involve real matrices.
   Complex matrices may be handled in a future version.
<P>
   Five fundamental matrix decompositions, which consist of pairs or triples
   of matrices, permutation vectors, and the like, produce results in five
   decomposition classes.  These decompositions are accessed by the Matrix
   class to compute solutions of simultaneous linear equations, determinants,
   inverses and other matrix functions.  The five decompositions are:
<P><UL>
   <LI>Cholesky Decomposition of symmetric, positive definite matrices.
   <LI>LU Decomposition of rectangular matrices.
   <LI>QR Decomposition of rectangular matrices.
   <LI>Singular Value Decomposition of rectangular matrices.
   <LI>Eigenvalue Decomposition of both symmetric and nonsymmetric square matrices.
</UL>
<DL>
<DT><B>Example of use:</B></DT>
<P>
<DD>Solve a linear system A x = b and compute the residual norm, ||b - A x||.
<P><PRE>
      double[][] vals = {{1.,2.,3},{4.,5.,6.},{7.,8.,10.}};
      Matrix A = new Matrix(vals);
      Matrix b = Matrix.random(3,1);
      Matrix x = A.solve(b);
      Matrix r = A.times(x).minus(b);
      double rnorm = r.normInf();
</PRE></DD>
</DL>

@author The MathWorks, Inc. and the National Institute of Standards and Technology.
@version 5 August 1998
*/

public class Matrix implements Cloneable, java.io.Serializable {

/* ------------------------
   Class variables
 * ------------------------ */

   /** Array for internal storage of elements.
   @serial internal array storage.
   */
   double[][] A;  // package-visible: the round-3 helper classes (MatrixOps/MatrixSolve/MatrixIo) share the storage

   /** Row and column dimensions.
   @serial row dimension.
   @serial column dimension.
   */
   int m, n;  // package-visible: the round-3 helper classes share the storage

/* ------------------------
   Constructors
 * ------------------------ */

   /** Construct an m-by-n matrix of zeros. 
   @param m    Number of rows.
   @param n    Number of colums.
   */

   public Matrix (int m, int n) {
      this.m = m;
      this.n = n;
      A = new double[m][n];
   }

   /** Construct an m-by-n constant matrix.
   @param m    Number of rows.
   @param n    Number of colums.
   @param s    Fill the matrix with this scalar value.
   */

   public Matrix (int m, int n, double s) {
      this.m = m;
      this.n = n;
      A = new double[m][n];
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            A[i][j] = s;
         }
      }
   }

   /** Construct a matrix from a 2-D array.
   @param A    Two-dimensional array of doubles.
   @exception  IllegalArgumentException All rows must have the same length
   @see        #constructWithCopy
   */

   public Matrix (double[][] A) {
      m = A.length;
      n = A[0].length;
      for (int i = 0; i < m; i++) {
         if (A[i].length != n) {
            throw new IllegalArgumentException("All rows must have the same length.");
         }
      }
      this.A = A;
   }

   /** Construct a matrix quickly without checking arguments.
   @param A    Two-dimensional array of doubles.
   @param m    Number of rows.
   @param n    Number of colums.
   */

   public Matrix (double[][] A, int m, int n) {
      this.A = A;
      this.m = m;
      this.n = n;
   }

   /** Construct a matrix from a one-dimensional packed array
   @param vals One-dimensional array of doubles, packed by columns (ala Fortran).
   @param m    Number of rows.
   @exception  IllegalArgumentException Array length must be a multiple of m.
   */

   public Matrix (double vals[], int m) {
      this.m = m;
      n = (m != 0 ? vals.length/m : 0);
      if (m*n != vals.length) {
         throw new IllegalArgumentException("Array length must be a multiple of m.");
      }
      A = new double[m][n];
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            A[i][j] = vals[i+j*m];
         }
      }
   }

/* ------------------------
   Public Methods
 * ------------------------ */

   /** Make a deep copy of a matrix
   */

   public Matrix copy () {
      Matrix X = new Matrix(m,n);
      double[][] C = X.getArray();
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            C[i][j] = A[i][j];
         }
      }
      return X;
   }

   /** Clone the Matrix object.
   */

   public Object clone () {
      return this.copy();
   }

   /** Access the internal two-dimensional array.
   @return     Pointer to the two-dimensional array of matrix elements.
   */

   public double[][] getArray () {
      return A;
   }

   /** Copy the internal two-dimensional array.
   @return     Two-dimensional array copy of matrix elements.
   */

   public double[][] getArrayCopy () {
      double[][] C = new double[m][n];
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            C[i][j] = A[i][j];
         }
      }
      return C;
   }

   /** Get row dimension.
   @return     m, the number of rows.
   */

   public int getRowDimension () {
      return m;
   }

   /** Get column dimension.
   @return     n, the number of columns.
   */

   public int getColumnDimension () {
      return n;
   }

   /** Get a single element.
   @param i    Row index.
   @param j    Column index.
   @return     A(i,j)
   @exception  ArrayIndexOutOfBoundsException
   */

   public double get (int i, int j) {
      return A[i][j];
   }

   /** Set a single element.
   @param i    Row index.
   @param j    Column index.
   @param s    A(i,j).
   @exception  ArrayIndexOutOfBoundsException
   */

   public void set (int i, int j, double s) {
      A[i][j] = s;
   }

   /** Matrix transpose.
   @return    A'
   */

   public Matrix transpose () {
      Matrix X = new Matrix(n,m);
      double[][] C = X.getArray();
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            C[j][i] = A[i][j];
         }
      }
      return X;
   }

      /** norm1: one-line delegator (scanaislop round-4 reshape; the
   upstream javadoc and body live in MatrixNorms). */
   public double norm1 () {
      return MatrixNorms.norm1(this);
   }

   /** Two norm
   @return    maximum singular value.
   */

   public double norm2 () {
      return (new SingularValueDecomposition(this).norm2());
   }

      /** normInf: one-line delegator (scanaislop round-4 reshape; the
   upstream javadoc and body live in MatrixNorms). */
   public double normInf () {
      return MatrixNorms.normInf(this);
   }

      /** normF: one-line delegator (scanaislop round-4 reshape; the
   upstream javadoc and body live in MatrixNorms). */
   public double normF () {
      return MatrixNorms.normF(this);
   }

   /** LU Decomposition
   @return     LUDecomposition
   @see LUDecomposition
   */

   public LUDecomposition lu () {
      return new LUDecomposition(this);
   }

   /** QR Decomposition
   @return     QRDecomposition
   @see QRDecomposition
   */

   public QRDecomposition qr () {
      return new QRDecomposition(this);
   }

   /** Cholesky Decomposition
   @return     CholeskyDecomposition
   @see CholeskyDecomposition
   */

   public CholeskyDecomposition chol () {
      return new CholeskyDecomposition(this);
   }

   /** Singular Value Decomposition
   @return     SingularValueDecomposition
   @see SingularValueDecomposition
   */

   public SingularValueDecomposition svd () {
      return new SingularValueDecomposition(this);
   }

   /** Eigenvalue Decomposition
   @return     EigenvalueDecomposition
   @see EigenvalueDecomposition
   */

   public EigenvalueDecomposition eig () {
      return new EigenvalueDecomposition(this);
   }


/* ------------------------
   Round-3 delegators (the bodies moved to the same-package helper
   classes MatrixOps/MatrixSolve/MatrixIo per the owner's scanaislop
   complexity order; the public API is unchanged)
 * ------------------------ */
   public Matrix getMatrix (int i0, int i1, int j0, int j1) {
      return MatrixOps.getMatrix(this, i0, i1, j0, j1);
   }
   public Matrix getMatrix (int[] r, int[] c) {
      return MatrixOps.getMatrix(this, r, c);
   }
   public Matrix getMatrix (int i0, int i1, int[] c) {
      return MatrixOps.getMatrix(this, i0, i1, c);
   }
   public Matrix getMatrix (int[] r, int j0, int j1) {
      return MatrixOps.getMatrix(this, r, j0, j1);
   }
   public Matrix plus (Matrix B) {
      return MatrixOps.plus(this, B);
   }
   public Matrix minus (Matrix B) {
      return MatrixOps.minus(this, B);
   }
   public Matrix times (double s) {
      return MatrixOps.times(this, s);
   }
   public Matrix times (Matrix B) {
      return MatrixOps.times(this, B);
   }
   public Matrix solve (Matrix B) {
      return MatrixSolve.solve(this, B);
   }
   public Matrix inverse () {
      return MatrixSolve.inverse(this);
   }
   public double det () {
      return MatrixSolve.det(this);
   }
   public int rank () {
      return MatrixSolve.rank(this);
   }
   public double cond () {
      return MatrixSolve.cond(this);
   }
   public void print (int w, int d) {
      MatrixIo.print(this, w, d);
   }
   public void print (PrintWriter output, int w, int d) {
      MatrixIo.print(this, output, w, d);
   }
   public void print (NumberFormat format, int width) {
      MatrixIo.print(this, format, width);
   }
   public void print (PrintWriter output, NumberFormat format, int width) {
      MatrixIo.print(this, output, format, width);
   }
   public static Matrix constructWithCopy (double[][] A) {
      return MatrixIo.constructWithCopy(A);
   }
   public static Matrix random (int m, int n) {
      return MatrixIo.random(m, n);
   }
   public static Matrix identity (int m, int n) {
      return MatrixIo.identity(m, n);
   }
/* ------------------------
   Round-4 delegators (the JamaCorpus driver's call graph, missing from
   the JamaBench-driven set above; same receiver-static mechanics)
 * ------------------------ */
   public void setMatrix (int i0, int i1, int j0, int j1, Matrix X) {
      MatrixOps.setMatrix(this, i0, i1, j0, j1, X);
   }
   public void setMatrix (int[] r, int[] c, Matrix X) {
      MatrixOps.setMatrix(this, r, c, X);
   }
   public Matrix uminus () {
      return MatrixOps.uminus(this);
   }
   public Matrix plusEquals (Matrix B) {
      return MatrixOps.plusEquals(this, B);
   }
   public Matrix minusEquals (Matrix B) {
      return MatrixOps.minusEquals(this, B);
   }
   public Matrix arrayTimes (Matrix B) {
      return MatrixOps.arrayTimes(this, B);
   }
   public Matrix arrayTimesEquals (Matrix B) {
      return MatrixOps.arrayTimesEquals(this, B);
   }
   public Matrix arrayRightDivide (Matrix B) {
      return MatrixOps.arrayRightDivide(this, B);
   }
   public Matrix arrayLeftDivide (Matrix B) {
      return MatrixOps.arrayLeftDivide(this, B);
   }
   public Matrix timesEquals (double s) {
      return MatrixOps.timesEquals(this, s);
   }
   public Matrix solveTranspose (Matrix B) {
      return MatrixSolve.solveTranspose(this, B);
   }
   public double trace () {
      return MatrixSolve.trace(this);
   }
   public double[] getColumnPackedCopy () {
      return MatrixIo.getColumnPackedCopy(this);
   }
   public double[] getRowPackedCopy () {
      return MatrixIo.getRowPackedCopy(this);
   }

  private static final long serialVersionUID = 1;
}
