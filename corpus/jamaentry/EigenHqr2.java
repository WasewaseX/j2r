package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: EigenHqr2 holds the nonsymmetric reduction from Hessenberg
 * to real Schur form (hqr2, 445 lines, flagged function-too-long AND
 * deep-nesting, plus cdiv), split from the single-file
 * EigenvalueDecomposition source per the owner's scanaislop complexity
 * order. The upstream locals live as the work-area fields; the bare
 * running eigenvalue index renames n -> nrun (upstream it shadowed the
 * dimension field; every dimension read inside hqr2 uses nn); the
 * phases are methods on the shared state. The eigenvector
 * backsubstitution lives in EigenHqr2Backsub and reaches this state
 * through the st receiver. Hand-maintained after this generation step.
 */

final class EigenHqr2 {
   double[][] H;           // the Hessenberg form (shared storage)
   double[][] V;           // the eigenvector accumulator (shared storage)
   double[] d;             // the real eigenvalue parts (shared storage)
   double[] e;             // the imaginary eigenvalue parts (shared storage)
   int n;                  // the matrix dimension (shared)

   // the upstream hqr2 locals, promoted to the work area (package-
   // visible: EigenHqr2Backsub reaches them through the st receiver)
   int nn, nrun, low, high, iter, l, m;
   boolean notlast;
   double eps, exshift, p, q, r, s, z, t, w, x, y, norm;
   double ra, sa, vr, vi;   // the complex-vector accumulators
   double cdivr, cdivi;    // the cdiv results (upstream transient pair)

   EigenHqr2(EigenvalueDecomposition ev) {
      H = ev.H;
      V = ev.V;
      d = ev.d;
      e = ev.e;
      n = ev.n;
   }

   void run () {
      hqr2();
   }

   // Nonsymmetric reduction from Hessenberg to real Schur form.
   //  This is derived from the Algol procedure hqr2, by Martin and
   //  Wilkinson, Handbook for Auto. Comp., Vol.ii-Linear Algebra, and
   //  the corresponding Fortran subroutine in EISPACK.

   private void hqr2 () {
   
      //  This is derived from the Algol procedure hqr2,
      //  by Martin and Wilkinson, Handbook for Auto. Comp.,
      //  Vol.ii-Linear Algebra, and the corresponding
      //  Fortran subroutine in EISPACK.
   
      // Initialize
   
      nn = n;
      nrun = nn-1;
      low = 0;
      high = nn-1;
      eps = Math.pow(2.0,-52.0);
      exshift = 0.0;
      p = 0;
      q = 0;
      r = 0;
      s = 0;
      z = 0;
   
      // Store roots isolated by balanc and compute matrix norm
   
      norm = 0.0;
      for (int i = 0; i < nn; i++) {
         if (i < low | i > high) {
            d[i] = H[i][i];
            e[i] = 0.0;
         }
         for (int j = Math.max(i-1,0); j < nn; j++) {
            norm = norm + Math.abs(H[i][j]);
         }
      }

      // Outer loop over eigenvalue index

      iter = 0;
      while (nrun >= low) {

         // Look for single small sub-diagonal element

         hqr2_find_l();

         // Check for convergence
         // One root found

         if (l == nrun) {
            H[nrun][nrun] = H[nrun][nrun] + exshift;
            d[nrun] = H[nrun][nrun];
            e[nrun] = 0.0;
            nrun--;
            iter = 0;

         // Two roots found

         } else if (l == nrun-1) {
            hqr2_two_roots();

         // No convergence yet

         } else {
            hqr2_shift();

            iter = iter + 1;   // (Could check iteration count here.)

            // Look for two consecutive small sub-diagonal elements

            hqr2_search_m();

            // Double QR step involving rows l:nrun and columns m:nrun

            hqr2_chase();
         }  // check convergence
      }  // while (nrun >= low)

      // Backsubstitute to find vectors of upper triangular form

      if (norm == 0.0) {
         return;
      }
      new EigenHqr2Backsub(this).run();

      // the isolated roots and the back transformation
      hqr2_finish();
   }

   // the small-subdiagonal search (was the upstream inner while)

   private void hqr2_find_l () {
         l = nrun;
         while (l > low) {
            s = Math.abs(H[l-1][l-1]) + Math.abs(H[l][l]);
            if (s == 0.0) {
               s = norm;
            }
            if (Math.abs(H[l][l-1]) < eps * s) {
               break;
            }
            l--;
         }
   }

   // Two roots found (was the upstream l == n-1 arm).

   private void hqr2_two_roots () {
            w = H[nrun][nrun-1] * H[nrun-1][nrun];
            p = (H[nrun-1][nrun-1] - H[nrun][nrun]) / 2.0;
            q = p * p + w;
            z = Math.sqrt(Math.abs(q));
            H[nrun][nrun] = H[nrun][nrun] + exshift;
            H[nrun-1][nrun-1] = H[nrun-1][nrun-1] + exshift;
            x = H[nrun][nrun];
   
            // Real pair
   
            if (q >= 0) {
               if (p >= 0) {
                  z = p + z;
               } else {
                  z = p - z;
               }
               d[nrun-1] = x + z;
               d[nrun] = d[nrun-1];
               if (z != 0.0) {
                  d[nrun] = x - w / z;
               }
               e[nrun-1] = 0.0;
               e[nrun] = 0.0;
               x = H[nrun][nrun-1];
               s = Math.abs(x) + Math.abs(z);
               p = x / s;
               q = z / s;
               r = Math.sqrt(p * p+q * q);
               p = p / r;
               q = q / r;
   
               // Row modification
   
               for (int j = nrun-1; j < nn; j++) {
                  z = H[nrun-1][j];
                  H[nrun-1][j] = q * z + p * H[nrun][j];
                  H[nrun][j] = q * H[nrun][j] - p * z;
               }
   
               // Column modification
   
               for (int i = 0; i <= nrun; i++) {
                  z = H[i][nrun-1];
                  H[i][nrun-1] = q * z + p * H[i][nrun];
                  H[i][nrun] = q * H[i][nrun] - p * z;
               }
   
               // Accumulate transformations
   
               for (int i = low; i <= high; i++) {
                  z = V[i][nrun-1];
                  V[i][nrun-1] = q * z + p * V[i][nrun];
                  V[i][nrun] = q * V[i][nrun] - p * z;
               }
   
            // Complex pair
   
            } else {
               d[nrun-1] = x + p;
               d[nrun] = x + p;
               e[nrun-1] = z;
               e[nrun] = -z;
            }
            nrun = nrun - 2;
            iter = 0;
   }

   // Form shift (the Wilkinson and MATLAB ad hoc shifts; was the head
   // of the upstream no-convergence arm).

   private void hqr2_shift () {
            // Form shift
   
            x = H[nrun][nrun];
            y = 0.0;
            w = 0.0;
            if (l < nrun) {
               y = H[nrun-1][nrun-1];
               w = H[nrun][nrun-1] * H[nrun-1][nrun];
            }
   
            // Wilkinson's original ad hoc shift
   
            if (iter == 10) {
               exshift += x;
               for (int i = low; i <= nrun; i++) {
                  H[i][i] -= x;
               }
               s = Math.abs(H[nrun][nrun-1]) + Math.abs(H[nrun-1][nrun-2]);
               x = y = 0.75 * s;
               w = -0.4375 * s * s;
            }

            // MATLAB's new ad hoc shift

            if (iter == 30) {
                s = (y - x) / 2.0;
                s = s * s + w;
                if (s > 0) {
                    s = Math.sqrt(s);
                    if (y < x) {
                       s = -s;
                    }
                    s = x - w / ((y - x) / 2.0 + s);
                    for (int i = low; i <= nrun; i++) {
                       H[i][i] -= s;
                    }
                    exshift += s;
                    x = y = w = 0.964;
                }
            }
   }

   // Look for two consecutive small sub-diagonal elements.

   private void hqr2_search_m () {
            m = nrun-2;
            while (m >= l) {
               z = H[m][m];
               r = x - z;
               s = y - z;
               p = (r * s - w) / H[m+1][m] + H[m][m+1];
               q = H[m+1][m+1] - z - r - s;
               r = H[m+2][m+1];
               s = Math.abs(p) + Math.abs(q) + Math.abs(r);
               p = p / s;
               q = q / s;
               r = r / s;
               if (m == l) {
                  break;
               }
               if (Math.abs(H[m][m-1]) * (Math.abs(q) + Math.abs(r)) <
                  eps * (Math.abs(p) * (Math.abs(H[m-1][m-1]) + Math.abs(z) +
                  Math.abs(H[m+1][m+1])))) {
                     break;
               }
               m--;
            }
   
            for (int i = m+2; i <= nrun; i++) {
               H[i][i-2] = 0.0;
               if (i > m+2) {
                  H[i][i-3] = 0.0;
               }
            }
   }

   // Double QR step involving rows l:n and columns m:n (the chase).

   private void hqr2_chase () {
            for (int k = m; k <= nrun-1; k++) {
               notlast = (k != nrun-1);
               if (k != m) {
                  p = H[k][k-1];
                  q = H[k+1][k-1];
                  r = (notlast ? H[k+2][k-1] : 0.0);
                  x = Math.abs(p) + Math.abs(q) + Math.abs(r);
                  if (x == 0.0) {
                      continue;
                  }
                  p = p / x;
                  q = q / x;
                  r = r / x;
               }

               s = Math.sqrt(p * p + q * q + r * r);
               if (p < 0) {
                  s = -s;
               }
               if (s != 0) {
                  if (k != m) {
                     H[k][k-1] = -s * x;
                  } else if (l != m) {
                     H[k][k-1] = -H[k][k-1];
                  }
                  p = p + s;
                  x = p / s;
                  y = q / s;
                  z = r / s;
                  q = q / p;
                  r = r / p;
   
                  // Row modification
   
                  for (int j = k; j < nn; j++) {
                     p = H[k][j] + q * H[k+1][j];
                     if (notlast) {
                        p = p + r * H[k+2][j];
                        H[k+2][j] = H[k+2][j] - p * z;
                     }
                     H[k][j] = H[k][j] - p * x;
                     H[k+1][j] = H[k+1][j] - p * y;
                  }
   
                  // Column modification
   
                  for (int i = 0; i <= Math.min(nrun,k+3); i++) {
                     p = x * H[i][k] + y * H[i][k+1];
                     if (notlast) {
                        p = p + z * H[i][k+2];
                        H[i][k+2] = H[i][k+2] - p * r;
                     }
                     H[i][k] = H[i][k] - p;
                     H[i][k+1] = H[i][k+1] - p * q;
                  }
   
                  // Accumulate transformations
   
                  for (int i = low; i <= high; i++) {
                     p = x * V[i][k] + y * V[i][k+1];
                     if (notlast) {
                        p = p + z * V[i][k+2];
                        V[i][k+2] = V[i][k+2] - p * r;
                     }
                     V[i][k] = V[i][k] - p;
                     V[i][k+1] = V[i][k+1] - p * q;
                  }
               }  // (s != 0)
            }  // k loop
   }

   // Vectors of isolated roots + the back transformation to the
   // eigenvectors of the original matrix.

   private void hqr2_finish () {
      // Vectors of isolated roots
   
      for (int i = 0; i < nn; i++) {
         if (i < low | i > high) {
            for (int j = i; j < nn; j++) {
               V[i][j] = H[i][j];
            }
         }
      }
   
      // Back transformation to get eigenvectors of original matrix
   
      for (int j = nn-1; j >= low; j--) {
         for (int i = low; i <= high; i++) {
            z = 0.0;
            for (int k = low; k <= Math.min(j,high); k++) {
               z = z + V[i][k] * H[k][j];
            }
            V[i][j] = z;
         }
      }
   }

   // Complex scalar division.

   void cdiv(double xr, double xi, double yr, double yi) {
      double r,d;
      if (Math.abs(yr) > Math.abs(yi)) {
         r = yi/yr;
         d = yr + r*yi;
         cdivr = (xr + r*xi)/d;
         cdivi = (xi - r*xr)/d;
      } else {
         r = yr/yi;
         d = yi + r*yr;
         cdivr = (r*xr + xi)/d;
         cdivi = (r*xi - xr)/d;
      }
   }
}
