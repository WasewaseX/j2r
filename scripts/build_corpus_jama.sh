#!/bin/bash
# Build the JAMA real-world corpus file: one .java, multiple package-private
# classes + a public JamaCorpus driver entry point.
# Source: NIST JAMA 1.0.3 (public domain), fetched to corpus/jama/.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=corpus/JamaCorpus.java
IMP=$(mktemp)
FRAG=$(mktemp)
DRV=$(mktemp)
trap 'rm -f "$IMP" "$FRAG" "$DRV"' EXIT

cat > "$DRV" <<'DRIVER'
/**
 * Differential driver over the JAMA library (real-world corpus, NIST
 * public-domain matrix package). Deterministic output only: every double
 * is printed via Double.toString; no Math.random, no NumberFormat paths.
 */
public class JamaCorpus {

  static void pm(String label, Matrix X) {
    System.out.println(label + " " + X.getRowDimension() + "x"
        + X.getColumnDimension());
    double[][] a = X.getArray();
    for (int i = 0; i < a.length; i++) {
      StringBuilder sb = new StringBuilder();
      for (int j = 0; j < a[i].length; j++) {
        if (j > 0) sb.append(' ');
        sb.append(Double.toString(a[i][j]));
      }
      System.out.println(sb.toString());
    }
  }

  static void pv(String label, double[] v) {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < v.length; i++) {
      if (i > 0) sb.append(' ');
      sb.append(Double.toString(v[i]));
    }
    System.out.println(label + " [" + sb.toString() + "]");
  }

  static void pi(String label, int[] v) {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < v.length; i++) {
      if (i > 0) sb.append(' ');
      sb.append(v[i]);
    }
    System.out.println(label + " [" + sb.toString() + "]");
  }

  public static void main(String[] args) {
    // --- construction and element access -------------------------------
    double[][] spd = {{4, 3, 0}, {3, 7, -1}, {0, -1, 9}};
    Matrix A = new Matrix(spd);
    pm("A", A);
    pm("I34", Matrix.identity(3, 4));
    pm("F23", new Matrix(2, 3, 2.5));
    pm("Z12", new Matrix(1, 2));
    Matrix packed = new Matrix(new double[]{1, 2, 3, 4, 5, 6}, 2);
    pm("packed2x3", packed);
    pv("colPacked", packed.getColumnPackedCopy());
    pv("rowPacked", packed.getRowPackedCopy());
    System.out.println("dims=" + A.getRowDimension() + "," + A.getColumnDimension());
    System.out.println("get=" + Double.toString(A.get(1, 2)));
    A.set(2, 2, 10.0);
    System.out.println("afterSet=" + Double.toString(A.get(2, 2)));
    A.set(2, 2, 9.0);
    try {
      A.get(5, 0);
    } catch (RuntimeException e) {
      System.out.println("oob: " + e.getMessage());
    }
    Matrix from2d = new Matrix(new double[][]{{1, 2}, {3, 4}});
    from2d.getArray()[0][0] = 99.0;
    System.out.println("ctorAlias=" + Double.toString(from2d.get(0, 0)));
    Matrix copyOf = Matrix.constructWithCopy(new double[][]{{1, 2}, {3, 4}});
    copyOf.getArray()[0][0] = 77.0;
    System.out.println("constructWithCopyIndependent="
        + Double.toString(copyOf.get(0, 0)));
    Matrix cp = A.copy();
    Object cl = A.clone();
    cp.set(0, 0, -1.0);
    System.out.println("copyIndependent=" + (A.get(0, 0) == 4.0)
        + " cloneIndependent=" + (((Matrix) cl).get(0, 0) == 4.0));
    Matrix sub = A.getMatrix(0, 1, 1, 2);
    pm("sub", sub);
    int[] r = {0, 2};
    int[] c = {1, 0};
    pm("subRC", A.getMatrix(r, c));
    pm("subRrange", A.getMatrix(r, 1, 2));
    pm("subCrange", A.getMatrix(1, 2, c));
    Matrix target = new Matrix(3, 3);
    target.setMatrix(0, 1, 1, 2, A.getMatrix(0, 1, 1, 2));
    target.setMatrix(r, c, A.getMatrix(r, c));
    pm("setMatrixTarget", target);
    try {
      A.getMatrix(0, 5, 0, 2);
    } catch (RuntimeException e) {
      System.out.println("submatrix: " + e.getMessage());
    }

    // --- arithmetic -----------------------------------------------------
    Matrix B = new Matrix(new double[][]{{1, 0, 2}, {0, 1, 1}, {1, 1, 0}});
    pm("plus", A.plus(B));
    pm("minus", A.minus(B));
    pm("uminus", B.uminus());
    pm("matmul", A.times(B));
    pm("scalar", B.times(2.5));
    pm("arrayTimes", A.arrayTimes(B));
    pm("arrayRightDivide", B.arrayRightDivide(B));
    pm("arrayLeftDivide", B.arrayLeftDivide(B));
    Matrix mut = B.copy();
    mut.plusEquals(B);
    pm("plusEquals", mut);
    mut.minusEquals(B);
    mut.arrayTimesEquals(B);
    mut.timesEquals(0.5);
    pm("chainMut", mut);
    pm("transpose", A.transpose());

    // --- norms ----------------------------------------------------------
    Matrix N34 = new Matrix(new double[][]{{1, -2, 3, 0}, {-4, 5, -6, 7}});
    System.out.println("norm1=" + Double.toString(N34.norm1())
        + " normInf=" + Double.toString(N34.normInf())
        + " normF=" + Double.toString(N34.normF())
        + " trace=" + Double.toString(A.trace()));

    // --- LU -------------------------------------------------------------
    LUDecomposition lu = A.lu();
    System.out.println("luNonsingular=" + lu.isNonsingular()
        + " det=" + Double.toString(A.det()));
    pm("L", lu.getL());
    pm("U", lu.getU());
    pi("pivot", lu.getPivot());
    pv("dpivot", lu.getDoublePivot());
    Matrix rhs = new Matrix(new double[][]{{
        1.0, 2.0, 3.0}, {0.5, -1.0, 4.0}, {2.0, 0.0, -1.0}});
    Matrix X = lu.solve(rhs);
    pm("luSolve", X);
    Matrix residual = A.times(X).minus(rhs);
    System.out.println("luResidual=" + Double.toString(residual.normInf()));
    pm("inverse", A.inverse());
    pm("solveTranspose", A.solveTranspose(rhs));
    Matrix hi = new Matrix(new double[][]{{1, 2}, {2, 4}});
    LUDecomposition luSing = hi.lu();
    System.out.println("singNonsingular=" + luSing.isNonsingular()
        + " singDet=" + Double.toString(hi.det()));
    try {
      luSing.solve(rhs.getMatrix(0, 1, 0, 0));
    } catch (RuntimeException e) {
      System.out.println("singular: " + e.getMessage());
    }

    // --- QR -------------------------------------------------------------
    QRDecomposition qr = A.qr();
    System.out.println("qrFullRank=" + qr.isFullRank());
    pm("Q", qr.getQ());
    pm("R", qr.getR());
    pm("H", qr.getH());
    Matrix qx = qr.solve(rhs.getMatrix(0, 2, 0, 0));
    pv("qrSolveCol", qx.getColumnPackedCopy());

    // --- Cholesky -------------------------------------------------------
    CholeskyDecomposition chol = A.chol();
    System.out.println("spd=" + chol.isSPD());
    pm("cholL", chol.getL());
    Matrix cx = chol.solve(rhs);
    pm("cholSolve", cx);
    System.out.println("cholResidual="
        + Double.toString(A.times(cx).minus(rhs).normInf()));
    Matrix nonSPD = new Matrix(new double[][]{{1, 2}, {2, 1}});
    CholeskyDecomposition cholBad = nonSPD.chol();
    System.out.println("badSpd=" + cholBad.isSPD());
    try {
      cholBad.solve(new Matrix(2, 1));
    } catch (RuntimeException e) {
      System.out.println("notspd: " + e.getMessage());
    }

    // --- SVD over the Hilbert matrix (classic ill-conditioned case) -----
    int n = 5;
    double[][] hh = new double[n][n];
    for (int i = 0; i < n; i++)
      for (int j = 0; j < n; j++)
        hh[i][j] = 1.0 / (double) (i + j + 1);
    Matrix H = new Matrix(hh);
    SingularValueDecomposition svd = H.svd();
    pv("singVals", svd.getSingularValues());
    System.out.println("norm2=" + Double.toString(svd.norm2())
        + " cond=" + Double.toString(svd.cond())
        + " rank=" + svd.rank());
    pm("S", svd.getS());
    pm("U", svd.getU());
    pm("V", svd.getV());
    System.out.println("mNorm2=" + Double.toString(H.norm2())
        + " mCond=" + Double.toString(H.cond())
        + " mRank=" + H.rank());

    // --- Eigen ----------------------------------------------------------
    EigenvalueDecomposition eig = A.eig();
    pv("eigReal", eig.getRealEigenvalues());
    pv("eigImag", eig.getImagEigenvalues());
    pm("V", eig.getV());
    pm("D", eig.getD());
    Matrix er = A.times(eig.getV()).minus(eig.getV().times(eig.getD()));
    System.out.println("eigResidual=" + Double.toString(er.normInf()));
    Matrix NS = new Matrix(new double[][]{
        {0, 11, -5}, {-2, 17, -7}, {-4, 26, -10}});
    EigenvalueDecomposition eigN = NS.eig();
    pv("nReal", eigN.getRealEigenvalues());
    pv("nImag", eigN.getImagEigenvalues());
    pm("nD", eigN.getD());
    Matrix nr = NS.times(eigN.getV())
        .minus(eigN.getV().times(eigN.getD()));
    System.out.println("nEigResidual=" + Double.toString(nr.normInf()));

    // --- packed-constructor error path ----------------------------------
    try {
      new Matrix(new double[]{1, 2, 3, 4, 5}, 2);
    } catch (RuntimeException e) {
      System.out.println("packed: " + e.getMessage());
    }

    // --- times dimension-mismatch error path ----------------------------
    try {
      A.times(new Matrix(2, 2));
    } catch (RuntimeException e) {
      System.out.println("dims: " + e.getMessage());
    }
  }
}
DRIVER

# Library files: drop package + intra-library imports; demote top-level
# public; split imports from bodies.
for f in corpus/jama/Matrix.java corpus/jama/CholeskyDecomposition.java \
         corpus/jama/EigenvalueDecomposition.java \
         corpus/jama/LUDecomposition.java corpus/jama/QRDecomposition.java \
         corpus/jama/SingularValueDecomposition.java corpus/jama/util/Maths.java; do
  grep -v '^import Jama\.' "$f"
done | sed -e 's/^package .*;//' \
    -e 's/^public \(final \|abstract \)\+\(class\|interface\|enum\)/\1\2/' \
    -e 's/^public \(class\|interface\|enum\)/\1/' | while IFS= read -r line; do
  case "$line" in
    import\ *) printf '%s\n' "$line" >> "$IMP" ;;
    *) printf '%s\n' "$line" >> "$FRAG" ;;
  esac
done

{
  sort -u "$IMP"
  cat "$DRV"
  cat "$FRAG"
} > "$OUT"

javac -d /tmp/jama_corpus_out "$OUT" && echo JAVAC_OK
