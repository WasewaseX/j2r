import jamaentry.*;

/**
 * Nonsymmetric-eigenvalue differential probe over the JAMA bench
 * family (corpus/jamaentry). The steady-state bench driver only eigs
 * symmetric matrices (S = A*A^T + N*I), so the orthes/hqr2/
 * backsubstitution path never runs on the gated workload; this probe
 * exercises the nonsymmetric branch deterministically: fixed xorshift
 * fills over several shapes, a rotation block that forces a complex
 * conjugate pair, a rank-deficient matrix (zero eigenvalues, the
 * norm==0 backsubstitution gate is not hit but the isolated-roots copy
 * is), an upper-triangular matrix (isolated roots end to end), and a
 * matrix whose Hessenberg form needs several QR iterations. Output is
 * a fixed set of deterministic lines (eigenvalue sums, imaginary-part
 * sums, eigenvector checksums) — byte-identical across channels.
 */
public class EigenNonsymProbe {

  static long seed = 20260101L;

  static double rnd() {
    seed ^= seed << 13;
    seed ^= seed >>> 7;
    seed ^= seed << 17;
    return ((seed >>> 11) * 2.3283064365386963e-10) - 0.5;
  }

  static Matrix fill(int n, double scale) {
    Matrix X = new Matrix(n, n);
    double[][] a = X.getArray();
    for (int i = 0; i < n; i++)
      for (int j = 0; j < n; j++)
        a[i][j] = rnd() * scale;
    return X;
  }

  static void report(String name, EigenvalueDecomposition eg, int n) {
    double[] d = eg.getRealEigenvalues();
    double[] e = eg.getImagEigenvalues();
    double dsum = 0.0;
    for (int i = 0; i < n; i++)
      dsum += d[i];
    double esum = 0.0;
    for (int i = 0; i < n; i++)
      esum += e[i];
    Matrix V = eg.getV();
    double vchk = 0.0;
    for (int i = 0; i < n; i++)
      for (int j = 0; j < n; j++)
        vchk += Math.abs(V.get(i, j)) * (i + 1) * (j + 1);
    Matrix D = eg.getD();
    double dchk = 0.0;
    for (int i = 0; i < n; i++)
      for (int j = 0; j < n; j++)
        dchk += Math.abs(D.get(i, j)) * (i + 3) * (j + 3);
    System.out.println(name + " dsum=" + Double.toString(dsum)
        + " esum=" + Double.toString(esum)
        + " vchk=" + Double.toString(vchk)
        + " dchk=" + Double.toString(dchk)
        + " nimag=" + nImag(e, n));
  }

  static int nImag(double[] e, int n) {
    int k = 0;
    for (int i = 0; i < n; i++)
      if (e[i] != 0.0)
        k++;
    return k;
  }

  // a 2x2 rotation block embedded at [r, r+1] forces a complex pair
  static Matrix withRotation(Matrix A, int r) {
    Matrix B = A.copy();
    double[][] b = B.getArray();
    double c = 0.6;
    double s = 0.8;
    b[r][r] = c;
    b[r][r + 1] = -s;
    b[r + 1][r] = s;
    b[r + 1][r + 1] = c;
    return B;
  }

  public static void main(String[] args) {
    // plain nonsymmetric squares at two scales
    Matrix A = fill(8, 4.0);
    report("plain8", A.eig(), 8);
    Matrix B = fill(13, 0.25);
    report("plain13", B.eig(), 13);

    // a forced complex pair plus a triangular tail
    Matrix C = withRotation(fill(10, 2.0), 3);
    report("rot10", C.eig(), 10);

    // rank-deficient: three identical rows, several zero eigenvalues
    Matrix D = new Matrix(9, 9);
    double[][] dd = D.getArray();
    Matrix F = fill(6, 3.0);
    double[][] ff = F.getArray();
    for (int i = 0; i < 6; i++)
      for (int j = 0; j < 9; j++)
        dd[i][j] = ff[i][j % 6];
    for (int i = 6; i < 9; i++)
      for (int j = 0; j < 9; j++)
        dd[i][j] = ff[2][j % 6];
    report("rankdef9", D.eig(), 9);

    // upper triangular: every root is isolated, the backsubstitution
    // takes the real branch with l == n throughout
    Matrix T = fill(7, 1.0);
    double[][] tt = T.getArray();
    for (int i = 0; i < 7; i++)
      for (int j = 0; j < i; j++)
        tt[i][j] = 0.0;
    tt[0][0] = 2.5;
    tt[1][1] = -1.25;
    report("tri7", T.eig(), 7);

    // a rotation block at the bottom-right corner of an otherwise
    // triangular matrix: the complex branch with l == n-1
    Matrix U = withRotation(T, 5);
    report("tri7rot", U.eig(), 7);

    // the bench-like shape: nonsymmetric 21x21 (the QR chase runs for
    // many iterations)
    Matrix G = fill(21, 1.5);
    report("plain21", G.eig(), 21);
    System.out.println("seedTail=" + seed);
  }
}
