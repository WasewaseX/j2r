package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: SingularValueCore holds the Golub-Reinsch singular value
 * algorithm (the upstream ~421-line constructor, flagged
 * function-too-long), split from the single-file
 * SingularValueDecomposition source per the owner's scanaislop
 * complexity order. The upstream constructor locals live as the
 * work-area fields, set at entry exactly where the upstream locals
 * lived; the phases are methods and the four kase bodies are
 * svd_case1..4 (their scratch locals stay case-local exactly as
 * upstream). Hand-maintained after this generation step.
 */

final class SingularValueCore {
   double[][] A;      // the working copy of the input matrix
   double[] e;        // the super-diagonal of the bidiagonal form
   double[] work;     // the row-transformation scratch (upstream
                              // ctor head)
   boolean wantu;     // generate U
   boolean wantv;     // generate V
   int nct, nrt, nu;  // the sweep bounds
   int p, pp;         // the live block size
   int iter;          // the QR-iteration counter
   double eps, tiny;  // the negligibility thresholds
   int k, kase, ks;   // the per-iteration selection state

   // the shared storage (package-visible fields of the decomposition)
   double[][] U;
   double[][] V;
   double[] s;
   int m, n;

   SingularValueCore(SingularValueDecomposition svd, double[][] acopy) {
      U = svd.U;
      V = svd.V;
      s = svd.s;
      m = svd.m;
      n = svd.n;
      A = acopy;
   }

   void run () {
      nu = Math.min(m,n);
      e = new double [n];
      work = new double [m];
      wantu = true;
      wantv = true;
      bidiagonalize();
      setUpFinal();
      generateU();
      generateV();
      iterate();
   }

   // Reduce A to bidiagonal form, storing the diagonal elements
   // in s and the super-diagonal elements in e.

   private void bidiagonalize () {
      // Reduce A to bidiagonal form, storing the diagonal elements
      // in s and the super-diagonal elements in e.

      nct = Math.min(m-1,n);
      nrt = Math.max(0,Math.min(n-2,m));
      for (int k = 0; k < Math.max(nct,nrt); k++) {
        if (k < nct) {
            svd_bidiag_col(k);
        }
         for (int j = k+1; j < n; j++) {
            if ((k < nct) & (s[k] != 0.0))  {

            // Apply the transformation.

               double t = 0;
               for (int i = k; i < m; i++) {
                  t += A[i][k]*A[i][j];
               }
               t = -t/A[k][k];
               for (int i = k; i < m; i++) {
                  A[i][j] += t*A[i][k];
               }
            }

            // Place the k-th row of A into e for the
            // subsequent calculation of the row transformation.

            e[j] = A[k][j];
         }
         if (wantu & (k < nct)) {

            // Place the transformation in U for subsequent back
            // multiplication.

            for (int i = k; i < m; i++) {
               U[i][k] = A[i][k];
            }
         }
        if (k < nrt) {
            svd_bidiag_row(k);
        }
    }
   }

   // Set up the final bidiagonal matrix or order p.

   private void setUpFinal () {
      // Set up the final bidiagonal matrix or order p.

      p = Math.min(n,m+1);
      if (nct < n) {
         s[nct] = A[nct][nct];
      }
      if (m < p) {
         s[p-1] = 0.0;
      }
      if (nrt+1 < p) {
         e[nrt] = A[nrt][p-1];
      }
      e[p-1] = 0.0;
   }

   // If required, generate U.

   private void generateU () {
      // If required, generate U.

      if (wantu) {
         for (int j = nct; j < nu; j++) {
            for (int i = 0; i < m; i++) {
               U[i][j] = 0.0;
            }
            U[j][j] = 1.0;
         }
         for (int k = nct-1; k >= 0; k--) {
            if (s[k] != 0.0) {
               uRotationApply(k);
               for (int i = k; i < m; i++ ) {
                  U[i][k] = -U[i][k];
               }
               U[k][k] = 1.0 + U[k][k];
               for (int i = 0; i < k-1; i++) {
                  U[i][k] = 0.0;
               }
            } else {
               for (int i = 0; i < m; i++) {
                  U[i][k] = 0.0;
               }
               U[k][k] = 1.0;
            }
         }
      }
   }

   // If required, generate V.

   private void generateV () {
      // If required, generate V.

      if (wantv) {
         for (int k = n-1; k >= 0; k--) {
            if ((k < nrt) & (e[k] != 0.0)) {
               vRotationApply(k);
            }
            for (int i = 0; i < n; i++) {
               V[i][k] = 0.0;
            }
            V[k][k] = 1.0;
         }
      }
   }

   // Main iteration loop for the singular values.

   private void iterate () {
      // Main iteration loop for the singular values.

      pp = p-1;
      iter = 0;
      eps = Math.pow(2.0,-52.0);
      tiny = Math.pow(2.0,-966.0);
      while (p > 0) {

         // Here is where a test for too many iterations would go.

         // This section of the program inspects for
         // negligible elements in the s and e arrays.  On
         // completion the variables kase and k are set as follows.

         // kase = 1     if s(p) and e[k-1] are negligible and k<p
         // kase = 2     if s(k) is negligible and k<p
         // kase = 3     if e[k-1] is negligible, k<p, and
         //              s(k), ..., s(p) are not negligible (qr step).
         // kase = 4     if e(p-1) is negligible (convergence).

         selectKase();
         k++;

         // Perform the task indicated by kase.

         switch (kase) {

            // Deflate negligible s(p).

            case 1: {
               SingularValueCases.case1(this);
            }
            break;

            // Split at negligible s(k).

            case 2: {
               SingularValueCases.case2(this);
            }
            break;

            // Perform one qr step.

            case 3: {
               SingularValueCases.case3(this);
            }
            break;

            // Convergence.

            case 4: {
               SingularValueCases.case4(this);
            }
            break;
         }
      }
   }

   // the kase selection of the upstream main loop (the two negligible-
   // element searches and the kase decision), extracted verbatim with the
   // work-area fields carrying k/ks/kase exactly where the upstream
   // locals lived (scanaislop round-4 reshape)

   private void selectKase () {
      for (k = p-2; k >= -1; k--) {
         if (k == -1) {
            break;
         }
         if (Math.abs(e[k]) <=
               tiny + eps*(Math.abs(s[k]) + Math.abs(s[k+1]))) {
            e[k] = 0.0;
            break;
         }
      }

      if (k == p-2) {
         kase = 4;
      } else {
         for (ks = p-1; ks >= k; ks--) {
            if (ks == k) {
               break;
            }
            double t = (ks != p ? Math.abs(e[ks]) : 0.) + 
                       (ks != k+1 ? Math.abs(e[ks-1]) : 0.);
            if (Math.abs(s[ks]) <= tiny + eps*t)  {
               s[ks] = 0.0;
               break;
            }
         }
         if (ks == k) {
            kase = 3;
         } else if (ks == p-1) {
            kase = 1;
         } else {
            kase = 2;
            k = ks;
         }
      }
   }

   // the U rotation application for the k-th column (was the inline
   // for-j block of the upstream generateU; scanaislop round-4 reshape)

   private void uRotationApply (int k) {
      for (int j = k+1; j < nu; j++) {
         double t = 0;
         for (int i = k; i < m; i++) {
            t += U[i][k]*U[i][j];
         }
         t = -t/U[k][k];
         for (int i = k; i < m; i++) {
            U[i][j] += t*U[i][k];
         }
      }
   }

   // the V rotation application for the k-th column (was the inline
   // for-j block of the upstream generateV; scanaislop round-4 reshape)

   private void vRotationApply (int k) {
      for (int j = k+1; j < nu; j++) {
         double t = 0;
         for (int i = k+1; i < n; i++) {
            t += V[i][k]*V[i][j];
         }
         t = -t/V[k+1][k];
         for (int i = k+1; i < n; i++) {
            V[i][j] += t*V[i][k];
         }
      }
   }

   // the k-th column Householder transformation (was the upstream
   // k < nct arm of the bidiagonalization sweep)

   private void svd_bidiag_col (int k) {

            // Compute the transformation for the k-th column and
            // place the k-th diagonal in s[k].
            // Compute 2-norm of k-th column without under/overflow.
            s[k] = 0;
            for (int i = k; i < m; i++) {
               s[k] = Maths.hypot(s[k],A[i][k]);
            }
            if (s[k] != 0.0) {
               if (A[k][k] < 0.0) {
                  s[k] = -s[k];
               }
               for (int i = k; i < m; i++) {
                  A[i][k] /= s[k];
               }
               A[k][k] += 1.0;
            }
            s[k] = -s[k];
   }

   // the k-th row transformation (was the upstream k < nrt arm; the
   // work array is a local exactly as upstream scoped)

   private void svd_bidiag_row (int k) {

            // Compute the k-th row transformation and place the
            // k-th super-diagonal in e[k].
            // Compute 2-norm without under/overflow.
            e[k] = 0;
            for (int i = k+1; i < n; i++) {
               e[k] = Maths.hypot(e[k],e[i]);
            }
            if (e[k] != 0.0) {
               if (e[k+1] < 0.0) {
                  e[k] = -e[k];
               }
               for (int i = k+1; i < n; i++) {
                  e[i] /= e[k];
               }
               e[k+1] += 1.0;
            }
            e[k] = -e[k];
            if ((k+1 < m) & (e[k] != 0.0)) {

            // Apply the transformation.

               for (int i = k+1; i < m; i++) {
                  work[i] = 0.0;
               }
               for (int j = k+1; j < n; j++) {
                  for (int i = k+1; i < m; i++) {
                     work[i] += e[j]*A[i][j];
                  }
               }
               for (int j = k+1; j < n; j++) {
                  double t = -e[j]/e[k+1];
                  for (int i = k+1; i < m; i++) {
                     A[i][j] += t*work[i];
                  }
               }
            }
            if (wantv) {

            // Place the transformation in V for subsequent
            // back multiplication.

               for (int i = k+1; i < n; i++) {
                  V[i][k] = e[i];
               }
            }
   }
}
