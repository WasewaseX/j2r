#!/bin/bash
# Build the JAMA steady-state bench corpus entry: the driver
# corpus/JamaBench.java plus the library entry sources
# corpus/jamaentry/*.java (package jamaentry).
#
# Round-3 (2026-11-18): the hosted scanaislop surface's complexity rules
# (file flagged above 441 lines, function above 88 lines, nesting max 5)
# apply to the single-file amalgam shape too, so the entry is now a
# driver plus a directory of per-class sources. The seven classes are
# GENERATED here from the vendored upstream tree (corpus/jama/ and
# corpus/jama/util/, package Jama — NIST public-domain JAMA 1.0.3, kept
# untouched as the provenance record) and the script reproduces the
# committed bytes byte-for-byte (REGEN-BYTE-IDENTICAL). The three files
# that still carry the owner's round-3 complexity findings (Matrix
# 1046 lines; EigenvalueDecomposition 956 lines with tred2/tql2/orthes/
# hqr2 flagged function-too-long/deep-nesting; SingularValueDecomposition
# 548 lines with the flagged SVD constructor) are reshaped in the
# following round-3 slices and then flip to HAND-maintained (verified
# for existence here, never overwritten) exactly like the JZlib entry's
# five state machines.
#
# The old single-file amalgam (driver + 7 classes, 3363 lines) is
# retired: corpus/JamaBench.java is the driver only and imports
# jamaentry.*. Class names inside jamaentry are unchanged from the
# amalgam, so the javac+java differential proof and the j2r
# three-channel comparison see the same program modulo the documented
# complexity reshaping.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=corpus/JamaBench.java
ENTRY=corpus/jamaentry
DRV=$(mktemp)
trap 'rm -f "$DRV"' EXIT

cat > "$DRV" <<'DRIVER'
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
DRIVER

# Library entry sources: the vendored file with its own package line
# replaced by the entry package and the intra-library Jama imports
# dropped (one flat package now); license and javadoc stay. The classes
# stay PUBLIC: in the one-file amalgam the old script had to demote
# them (a .java file can carry only one public top-level class), but a
# per-class source file is named after its class, so public is legal
# again — and the default-package driver imports jamaentry.*, which
# cannot see package-private types across the package boundary. The
# Jama vendored files are space-indented already — no tab expansion
# needed (unlike the JZlib ones).
mkdir -p "$ENTRY"
for f in corpus/jama/Matrix.java corpus/jama/CholeskyDecomposition.java \
         corpus/jama/EigenvalueDecomposition.java \
         corpus/jama/LUDecomposition.java corpus/jama/QRDecomposition.java \
         corpus/jama/SingularValueDecomposition.java corpus/jama/util/Maths.java; do
  cls=$(basename "$f" .java)
  {
    echo "package jamaentry;"
    echo
    grep -v '^package ' "$f" | grep -v '^import Jama\.'
  } > "$ENTRY/$cls.java"
done

{
  echo "import jamaentry.*;"
  echo
  cat "$DRV"
} > "$OUT"

javac -sourcepath corpus -d /tmp/jamabench-bin "$OUT" && echo "JAVAC_OK: $OUT + $ENTRY/*"
