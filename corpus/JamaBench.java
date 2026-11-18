import jamaentry.*;

/**
 * Steady-state bench driver over the JAMA library (real-world corpus,
 * NIST public-domain matrix package). Deterministic output only: a fixed
 * xorshift64 seed fills every matrix, no Math.random, no locale/number
 * formatting paths. Each iteration runs the full decomposition battery
 * (LU, QR, Cholesky on an SPD product, symmetric Eigen, rectangular SVD,
 * LU solve) over a 40x40 / 40x24 problem, so the hot loops are real
 * dense-numeric Java, not synthetic kernel stubs.
 */
public class JamaBench {

  static long seed = 88172645463325252L;

  static double rnd() {
    seed ^= seed << 13;
    seed ^= seed >>> 7;
    seed ^= seed << 17;
    // top 53 bits scaled to [-0.5, 0.5): uniform, deterministic
    return ((seed >>> 11) * 2.3283064365386963e-10) - 0.5;
  }

  static Matrix fill(int m, int n) {
    Matrix X = new Matrix(m, n);
    double[][] a = X.getArray();
    for (int i = 0; i < m; i++)
      for (int j = 0; j < n; j++)
        a[i][j] = rnd() * 8.0;
    return X;
  }

  public static void main(String[] args) {
    final int K = 1200; // iterations of the decomposition battery
    final int N = 40;  // square problem size
    final int M = 24;  // SVD column count (N x M, m >= n required)
    double acc = 0.0;
    for (int k = 0; k < K; k++) {
      Matrix A = fill(N, N);

      LUDecomposition lu = A.lu();
      acc += Math.abs(lu.getU().get(0, 0)) + lu.getL().normF();

      QRDecomposition qr = A.qr();
      acc += Math.abs(qr.getR().get(0, 0)) + qr.getQ().normF();

      // S = A*A^T + N*I is strictly positive definite by construction
      // (A*A^T is PSD and +N*I lifts every eigenvalue above N).
      Matrix S = A.times(A.transpose())
          .plus(Matrix.identity(N, N).times((double) N));
      CholeskyDecomposition ch = S.chol();
      acc += ch.getL().normF();

      EigenvalueDecomposition eg = S.eig();
      acc += Math.abs(eg.getRealEigenvalues()[0]) + eg.getV().normF();

      SingularValueDecomposition sv = fill(N, M).svd();
      acc += Math.abs(sv.getSingularValues()[0]) + sv.getV().normF();

      Matrix X = lu.solve(fill(N, 3));
      acc += Math.abs(X.get(0, 0));
    }
    System.out.println("K=" + K + " N=" + N + " M=" + M);
    System.out.println("acc=" + Double.toString(acc));
    System.out.println("seedTail=" + seed);
  }
}
