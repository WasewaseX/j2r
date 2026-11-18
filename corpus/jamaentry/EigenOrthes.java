package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: EigenOrthes holds the nonsymmetric reduction to Hessenberg
 * form (orthes, 94 lines, flagged function-too-long), split from the
 * single-file EigenvalueDecomposition source per the owner's
 * scanaislop complexity order. The upstream locals live as the
 * work-area fields; the moved bodies are the upstream bodies
 * byte-faithful. Hand-maintained after this generation step.
 */

final class EigenOrthes {
   private double[][] H;   // the Hessenberg form (shared storage)
   private double[][] V;   // the eigenvector accumulator (shared storage)
   private double[] ort;   // the working storage for the reduction
   private int n;          // the matrix dimension (shared)

   // the upstream orthes locals, promoted to the work area
   private int low, high;
   private double scale, h, g, f;

   EigenOrthes(EigenvalueDecomposition ev) {
      H = ev.H;
      V = ev.V;
      ort = ev.ort;
      n = ev.n;
   }

   void run () {
      orthes_reduce();
      orthes_accum();
   }

   // Nonsymmetric reduction to Hessenberg form (the Householder sweep;
   // upstream orthes split at the accumulate boundary).

   private void orthes_reduce () {
   
      //  This is derived from the Algol procedures orthes and ortran,
      //  by Martin and Wilkinson, Handbook for Auto. Comp.,
      //  Vol.ii-Linear Algebra, and the corresponding
      //  Fortran subroutines in EISPACK.
   
      low = 0;
      high = n-1;
   
      for (int m = low+1; m <= high-1; m++) {
   
         // Scale column.
   
         scale = 0.0;
         for (int i = m; i <= high; i++) {
            scale = scale + Math.abs(H[i][m-1]);
         }
         if (scale != 0.0) {
   
            // Compute Householder transformation.
   
            h = 0.0;
            for (int i = high; i >= m; i--) {
               ort[i] = H[i][m-1]/scale;
               h += ort[i] * ort[i];
            }
            g = Math.sqrt(h);
            if (ort[m] > 0) {
               g = -g;
            }
            h = h - ort[m] * g;
            ort[m] = ort[m] - g;
   
            // Apply Householder similarity transformation
            // H = (I-u*u'/h)*H*(I-u*u')/h)
   
            for (int j = m; j < n; j++) {
               f = 0.0;
               for (int i = high; i >= m; i--) {
                  f += ort[i]*H[i][j];
               }
               f = f/h;
               for (int i = m; i <= high; i++) {
                  H[i][j] -= f*ort[i];
               }
           }
   
           for (int i = 0; i <= high; i++) {
               f = 0.0;
               for (int j = high; j >= m; j--) {
                  f += ort[j]*H[i][j];
               }
               f = f/h;
               for (int j = m; j <= high; j++) {
                  H[i][j] -= f*ort[j];
               }
            }
            ort[m] = scale*ort[m];
            H[m][m-1] = scale*g;
         }
      }
   }

   // Accumulate transformations (Algol's ortran).

   private void orthes_accum () {
      // Accumulate transformations (Algol's ortran).

      for (int i = 0; i < n; i++) {
         for (int j = 0; j < n; j++) {
            V[i][j] = (i == j ? 1.0 : 0.0);
         }
      }

      for (int m = high-1; m >= low+1; m--) {
         if (H[m][m-1] != 0.0) {
            for (int i = m+1; i <= high; i++) {
               ort[i] = H[i][m-1];
            }
            for (int j = m; j <= high; j++) {
               g = 0.0;
               for (int i = m; i <= high; i++) {
                  g += ort[i] * V[i][j];
               }
               // Double division avoids possible underflow
               g = (g / ort[m]) / H[m][m-1];
               for (int i = m; i <= high; i++) {
                  V[i][j] += g * ort[i];
               }
            }
         }
      }
   }
}
