#!/bin/bash
# Build the JAMA steady-state bench corpus file: one .java, multiple
# package-private classes + a public JamaBench driver entry point.
# Same mechanical merge as build_corpus_jama.sh (NIST JAMA 1.0.3, public
# domain, vendored verbatim under corpus/jama/) with a bench-only driver.
#
# Purpose (R5 steady-state gap): a real-library workload long enough that
# the full tiered JVM (C1 -> C2) reaches steady state — startup and JIT
# warmup become noise — so the native-vs-JIT hot-loop comparison is
# honest. Output is a fixed handful of deterministic lines (fixed xorshift
# seed, no Math.random), so the same stem can later join the differential
# corpus suite unchanged.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=corpus/JamaBench.java
IMP=$(mktemp)
FRAG=$(mktemp)
DRV=$(mktemp)
trap 'rm -f "$IMP" "$FRAG" "$DRV"' EXIT

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

# Library files: drop package + intra-library imports; demote top-level
# public; split imports from bodies. (Identical merge to the JAMA wave.)
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

rm -rf /tmp/jamabench_out && javac -d /tmp/jamabench_out "$OUT" && echo JAVAC_OK
