package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: EigenSymmetric holds the symmetric branch of the JAMA
 * eigenvalue decomposition (tred2 117 lines and tql2 120 lines, both
 * flagged function-too-long), split from the single-file
 * EigenvalueDecomposition source per the owner's scanaislop complexity
 * order. The upstream locals live as the work-area fields, set at
 * entry exactly where the upstream locals lived; the moved bodies are
 * the upstream bodies byte-faithful. Hand-maintained after this
 * generation step.
 */

final class EigenSymmetric {
   private double[][] V;   // the eigenvector accumulator (shared storage)
   private double[] d;     // the real eigenvalue parts (shared storage)
   private double[] e;     // the imaginary eigenvalue parts (shared storage)
   private int n;          // the matrix dimension (shared)

   // the upstream tred2/tql2 locals, promoted to the work area
   private double scale, h, f, g, hh, tst1, eps, p, r, dl1;
   private double c, c2, c3, el1, s, s2;
   private int m, iter, k;

   EigenSymmetric(EigenvalueDecomposition ev) {
      V = ev.V;
      d = ev.d;
      e = ev.e;
      n = ev.n;
   }

   void run () {
      tred2();
      tql2();
   }

   // Symmetric Householder reduction to tridiagonal form.

   private void tred2 () {
      //  This is derived from the Algol procedures tred2 by
      //  Bowdler, Martin, Reinsch, and Wilkinson, Handbook for
      //  Auto. Comp., Vol.ii-Linear Algebra, and the corresponding
      //  Fortran subroutine in EISPACK.


   //  This is derived from the Algol procedures tred2 by
   //  Bowdler, Martin, Reinsch, and Wilkinson, Handbook for
   //  Auto. Comp., Vol.ii-Linear Algebra, and the corresponding
   //  Fortran subroutine in EISPACK.

      for (int j = 0; j < n; j++) {
         d[j] = V[n-1][j];
      }

      // Householder reduction to tridiagonal form.
   
      for (int i = n-1; i > 0; i--) {
   
         // Scale to avoid under/overflow.
   
         scale = 0.0;
         h = 0.0;
         for (int k = 0; k < i; k++) {
            scale = scale + Math.abs(d[k]);
         }
         if (scale == 0.0) {
            e[i] = d[i-1];
            for (int j = 0; j < i; j++) {
               d[j] = V[i-1][j];
               V[i][j] = 0.0;
               V[j][i] = 0.0;
            }
         } else {
            tred2_householder(i);
         }
         d[i] = h;
      }
      tred2_accum();
   }

   // the Householder-vector branch of the reduction loop (was the
   // upstream else arm, extracted to keep every function under the
   // 80-line house limit)

   private void tred2_householder (int i) {
   
            // Generate Householder vector.
   
            for (int k = 0; k < i; k++) {
               d[k] /= scale;
               h += d[k] * d[k];
            }
            f = d[i-1];
            g = Math.sqrt(h);
            if (f > 0) {
               g = -g;
            }
            e[i] = scale * g;
            h = h - f * g;
            d[i-1] = f - g;
            for (int j = 0; j < i; j++) {
               e[j] = 0.0;
            }
   
            // Apply similarity transformation to remaining columns.
   
            for (int j = 0; j < i; j++) {
               f = d[j];
               V[j][i] = f;
               g = e[j] + V[j][j] * f;
               for (int k = j+1; k <= i-1; k++) {
                  g += V[k][j] * d[k];
                  e[k] += V[k][j] * f;
               }
               e[j] = g;
            }
            f = 0.0;
            for (int j = 0; j < i; j++) {
               e[j] /= h;
               f += e[j] * d[j];
            }
            hh = f / (h + h);
            for (int j = 0; j < i; j++) {
               e[j] -= hh * d[j];
            }
            for (int j = 0; j < i; j++) {
               f = d[j];
               g = e[j];
               for (int k = j; k <= i-1; k++) {
                  V[k][j] -= (f * e[k] + g * d[k]);
               }
               d[j] = V[i-1][j];
               V[i][j] = 0.0;
            }
   }

   // Accumulate transformations.

   private void tred2_accum () {
      // Accumulate transformations.
   
      for (int i = 0; i < n-1; i++) {
         V[n-1][i] = V[i][i];
         V[i][i] = 1.0;
         h = d[i+1];
         if (h != 0.0) {
            for (int k = 0; k <= i; k++) {
               d[k] = V[k][i+1] / h;
            }
            for (int j = 0; j <= i; j++) {
               g = 0.0;
               for (int k = 0; k <= i; k++) {
                  g += V[k][i+1] * V[k][j];
               }
               for (int k = 0; k <= i; k++) {
                  V[k][j] -= g * d[k];
               }
            }
         }
         for (int k = 0; k <= i; k++) {
            V[k][i+1] = 0.0;
         }
      }
      for (int j = 0; j < n; j++) {
         d[j] = V[n-1][j];
         V[n-1][j] = 0.0;
      }
      V[n-1][n-1] = 1.0;
      e[0] = 0.0;
   }

   // Symmetric tridiagonal QL algorithm.

   private void tql2 () {
      //  This is derived from the Algol procedures tql2, by
      //  Bowdler, Martin, Reinsch, and Wilkinson, Handbook for
      //  Auto. Comp., Vol.ii-Linear Algebra, and the corresponding
      //  Fortran subroutine in EISPACK.


   //  This is derived from the Algol procedures tql2, by
   //  Bowdler, Martin, Reinsch, and Wilkinson, Handbook for
   //  Auto. Comp., Vol.ii-Linear Algebra, and the corresponding
   //  Fortran subroutine in EISPACK.
   
      for (int i = 1; i < n; i++) {
         e[i-1] = e[i];
      }
      e[n-1] = 0.0;
   
      f = 0.0;
      tst1 = 0.0;
      eps = Math.pow(2.0,-52.0);
      for (int l = 0; l < n; l++) {

         // Find small subdiagonal element
   
         tst1 = Math.max(tst1,Math.abs(d[l]) + Math.abs(e[l]));
         m = l;
         while (m < n) {
            if (Math.abs(e[m]) <= eps*tst1) {
               break;
            }
            m++;
         }
   
         // If m == l, d[l] is an eigenvalue,
         // otherwise, iterate.
   
         if (m > l) {
            iter = 0;
            tql2_iterate(l);
         }
         d[l] = d[l] + f;
         e[l] = 0.0;
      }
     
      // Sort eigenvalues and corresponding vectors.
   
      for (int i = 0; i < n-1; i++) {
         int k = i;
         double p = d[i];
         for (int j = i+1; j < n; j++) {
            if (d[j] < p) {
               k = j;
               p = d[j];
            }
         }
         if (k != i) {
            d[k] = d[i];
            d[i] = p;
            for (int j = 0; j < n; j++) {
               p = V[j][i];
               V[j][i] = V[j][k];
               V[j][k] = p;
            }
         }
      }
   }

   // the QL iteration on one eigenvalue (was the upstream do-while
   // under the m > l convergence arm)

   private void tql2_iterate (int l) {
            do {
               iter = iter + 1;  // (Could check iteration count here.)
   
               // Compute implicit shift
   
               g = d[l];
               p = (d[l+1] - g) / (2.0 * e[l]);
               r = Maths.hypot(p,1.0);
               if (p < 0) {
                  r = -r;
               }
               d[l] = e[l] / (p + r);
               d[l+1] = e[l] * (p + r);
               dl1 = d[l+1];
               h = g - d[l];
               for (int i = l+2; i < n; i++) {
                  d[i] -= h;
               }
               f = f + h;
   
               // Implicit QL transformation.
   
               p = d[m];
               c = 1.0;
               c2 = c;
               c3 = c;
               el1 = e[l+1];
               s = 0.0;
               s2 = 0.0;
               for (int i = m-1; i >= l; i--) {
                  c3 = c2;
                  c2 = c;
                  s2 = s;
                  g = c * e[i];
                  h = c * p;
                  r = Maths.hypot(p,e[i]);
                  e[i+1] = s * r;
                  s = e[i] / r;
                  c = p / r;
                  p = c * d[i] - s * g;
                  d[i+1] = h + s * (c * g + s * d[i]);
   
                  // Accumulate transformation.
   
                  for (int k = 0; k < n; k++) {
                     h = V[k][i+1];
                     V[k][i+1] = s * V[k][i] + c * h;
                     V[k][i] = c * V[k][i] - s * h;
                  }
               }
               p = -s * s2 * c3 * el1 * e[l] / dl1;
               e[l] = s * p;
               d[l] = c * p;
   
               // Check for convergence.
   
            } while (Math.abs(e[l]) > eps*tst1);
   }
}
