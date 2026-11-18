package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: MatrixOps holds the block getMatrix/setMatrix family and the element algebra (plus/minus/array/times families) of the JAMA Matrix, split from the
 * single-file Matrix source per the owner's scanaislop complexity
 * order (file above 441 lines). The moved bodies are the upstream
 * bodies with the receiver made explicit where the file split
 * requires it (the receiver is named mx so the bare upstream field
 * reads A/m/n become mx.A/mx.m/mx.n) - behavior byte-identical,
 * pinned by the javac+java stdout proof and the j2r three-channel
 * gate. Hand-maintained after this generation step.
 */

final class MatrixOps {

   /** Get a submatrix.
   @param i0   Initial row index
   @param i1   Final row index
   @param j0   Initial column index
   @param j1   Final column index
   @return     A(i0:i1,j0:j1)
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static Matrix getMatrix (Matrix mx, int i0, int i1, int j0, int j1) {
      Matrix X = new Matrix(i1-i0+1,j1-j0+1);
      double[][] B = X.getArray();
      try {
         for (int i = i0; i <= i1; i++) {
            for (int j = j0; j <= j1; j++) {
               B[i-i0][j-j0] = mx.A[i][j];
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
      return X;
   }

   /** Get a submatrix.
   @param r    Array of row indices.
   @param c    Array of column indices.
   @return     A(r(:),c(:))
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static Matrix getMatrix (Matrix mx, int[] r, int[] c) {
      Matrix X = new Matrix(r.length,c.length);
      double[][] B = X.getArray();
      try {
         for (int i = 0; i < r.length; i++) {
            for (int j = 0; j < c.length; j++) {
               B[i][j] = mx.A[r[i]][c[j]];
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
      return X;
   }

   /** Get a submatrix.
   @param i0   Initial row index
   @param i1   Final row index
   @param c    Array of column indices.
   @return     A(i0:i1,c(:))
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static Matrix getMatrix (Matrix mx, int i0, int i1, int[] c) {
      Matrix X = new Matrix(i1-i0+1,c.length);
      double[][] B = X.getArray();
      try {
         for (int i = i0; i <= i1; i++) {
            for (int j = 0; j < c.length; j++) {
               B[i-i0][j] = mx.A[i][c[j]];
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
      return X;
   }

   /** Get a submatrix.
   @param r    Array of row indices.
   @param j0   Initial column index
   @param j1   Final column index
   @return     A(r(:),j0:j1)
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static Matrix getMatrix (Matrix mx, int[] r, int j0, int j1) {
      Matrix X = new Matrix(r.length,j1-j0+1);
      double[][] B = X.getArray();
      try {
         for (int i = 0; i < r.length; i++) {
            for (int j = j0; j <= j1; j++) {
               B[i][j-j0] = mx.A[r[i]][j];
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
      return X;
   }

   /** Set a submatrix.
   @param i0   Initial row index
   @param i1   Final row index
   @param j0   Initial column index
   @param j1   Final column index
   @param X    A(i0:i1,j0:j1)
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static void setMatrix (Matrix mx, int i0, int i1, int j0, int j1, Matrix X) {
      try {
         for (int i = i0; i <= i1; i++) {
            for (int j = j0; j <= j1; j++) {
               mx.A[i][j] = X.get(i-i0,j-j0);
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
   }

   /** Set a submatrix.
   @param r    Array of row indices.
   @param c    Array of column indices.
   @param X    A(r(:),c(:))
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static void setMatrix (Matrix mx, int[] r, int[] c, Matrix X) {
      try {
         for (int i = 0; i < r.length; i++) {
            for (int j = 0; j < c.length; j++) {
               mx.A[r[i]][c[j]] = X.get(i,j);
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
   }

   /** Set a submatrix.
   @param r    Array of row indices.
   @param j0   Initial column index
   @param j1   Final column index
   @param X    A(r(:),j0:j1)
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static void setMatrix (Matrix mx, int[] r, int j0, int j1, Matrix X) {
      try {
         for (int i = 0; i < r.length; i++) {
            for (int j = j0; j <= j1; j++) {
               mx.A[r[i]][j] = X.get(i,j-j0);
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
   }

   /** Set a submatrix.
   @param i0   Initial row index
   @param i1   Final row index
   @param c    Array of column indices.
   @param X    A(i0:i1,c(:))
   @exception  ArrayIndexOutOfBoundsException Submatrix indices
   */

   static void setMatrix (Matrix mx, int i0, int i1, int[] c, Matrix X) {
      try {
         for (int i = i0; i <= i1; i++) {
            for (int j = 0; j < c.length; j++) {
               mx.A[i][c[j]] = X.get(i-i0,j);
            }
         }
      } catch(ArrayIndexOutOfBoundsException e) {
         throw new ArrayIndexOutOfBoundsException("Submatrix indices");
      }
   }

   /**  Unary minus
   @return    -A
   */

   static Matrix uminus (Matrix mx) {
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = -mx.A[i][j];
         }
      }
      return X;
   }

   /** C = A + B
   @param B    another matrix
   @return     A + B
   */

   static Matrix plus (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = mx.A[i][j] + B.A[i][j];
         }
      }
      return X;
   }

   /** A = A + B
   @param B    another matrix
   @return     A + B
   */

   static Matrix plusEquals (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            mx.A[i][j] = mx.A[i][j] + B.A[i][j];
         }
      }
      return mx;
   }

   /** C = A - B
   @param B    another matrix
   @return     A - B
   */

   static Matrix minus (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = mx.A[i][j] - B.A[i][j];
         }
      }
      return X;
   }

   /** A = A - B
   @param B    another matrix
   @return     A - B
   */

   static Matrix minusEquals (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            mx.A[i][j] = mx.A[i][j] - B.A[i][j];
         }
      }
      return mx;
   }

   /** Element-by-element multiplication, C = A.*B
   @param B    another matrix
   @return     A.*B
   */

   static Matrix arrayTimes (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = mx.A[i][j] * B.A[i][j];
         }
      }
      return X;
   }

   /** Element-by-element multiplication in place, A = A.*B
   @param B    another matrix
   @return     A.*B
   */

   static Matrix arrayTimesEquals (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            mx.A[i][j] = mx.A[i][j] * B.A[i][j];
         }
      }
      return mx;
   }

   /** Element-by-element right division, C = A./B
   @param B    another matrix
   @return     A./B
   */

   static Matrix arrayRightDivide (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = mx.A[i][j] / B.A[i][j];
         }
      }
      return X;
   }

   /** Element-by-element right division in place, A = A./B
   @param B    another matrix
   @return     A./B
   */

   static Matrix arrayRightDivideEquals (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            mx.A[i][j] = mx.A[i][j] / B.A[i][j];
         }
      }
      return mx;
   }

   /** Element-by-element left division, C = A.\B
   @param B    another matrix
   @return     A.\B
   */

   static Matrix arrayLeftDivide (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = B.A[i][j] / mx.A[i][j];
         }
      }
      return X;
   }

   /** Element-by-element left division in place, A = A.\B
   @param B    another matrix
   @return     A.\B
   */

   static Matrix arrayLeftDivideEquals (Matrix mx, Matrix B) {
      checkMatrixDimensions(mx, B);
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            mx.A[i][j] = B.A[i][j] / mx.A[i][j];
         }
      }
      return mx;
   }

   /** Multiply a matrix by a scalar, C = s*A
   @param s    scalar
   @return     s*A
   */

   static Matrix times (Matrix mx, double s) {
      Matrix X = new Matrix(mx.m,mx.n);
      double[][] C = X.getArray();
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            C[i][j] = s*mx.A[i][j];
         }
      }
      return X;
   }

   /** Multiply a matrix by a scalar in place, A = s*A
   @param s    scalar
   @return     replace A by s*A
   */

   static Matrix timesEquals (Matrix mx, double s) {
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            mx.A[i][j] = s*mx.A[i][j];
         }
      }
      return mx;
   }

   /** Linear algebraic matrix multiplication, A * B
   @param B    another matrix
   @return     Matrix product, A * B
   @exception  IllegalArgumentException Matrix inner dimensions must agree.
   */

   static Matrix times (Matrix mx, Matrix B) {
      if (B.m != mx.n) {
         throw new IllegalArgumentException("Matrix inner dimensions must agree.");
      }
      Matrix X = new Matrix(mx.m,B.n);
      double[][] C = X.getArray();
      double[] Bcolj = new double[mx.n];
      for (int j = 0; j < B.n; j++) {
         for (int k = 0; k < mx.n; k++) {
            Bcolj[k] = B.A[k][j];
         }
         for (int i = 0; i < mx.m; i++) {
            double[] Arowi = mx.A[i];
            double s = 0;
            for (int k = 0; k < mx.n; k++) {
               s += Arowi[k]*Bcolj[k];
            }
            C[i][j] = s;
         }
      }
      return X;
   }


   /** Check if size(A) == size(B) **/

   static void checkMatrixDimensions (Matrix mx, Matrix B) {
      if (B.m != mx.m || B.n != mx.n) {
         throw new IllegalArgumentException("Matrix dimensions must agree.");
      }
   }

  private MatrixOps() {}
}
