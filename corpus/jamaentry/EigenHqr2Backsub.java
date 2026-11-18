package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: EigenHqr2Backsub holds the eigenvector backsubstitution of
 * the nonsymmetric eigenvalue decomposition (the tail of upstream
 * hqr2, ~134 lines, part of the flagged function-too-long finding),
 * split from the single-file EigenvalueDecomposition source per the
 * owner's scanaislop complexity order. It reads the EigenHqr2 work
 * area through the st receiver; the bare upstream local reads spell
 * st.<field>. Hand-maintained after this generation step.
 */

final class EigenHqr2Backsub {
   private final EigenHqr2 st;

   EigenHqr2Backsub(EigenHqr2 state) {
      st = state;
   }

   // Backsubstitute to find vectors of upper triangular form.

   void run () {
      for (st.nrun = st.nn-1; st.nrun >= 0; st.nrun--) {
         st.p = st.d[st.nrun];
         st.q = st.e[st.nrun];

         if (st.q == 0) {
            backsub_real();

         // Complex vector

         } else if (st.q < 0) {
            backsub_complex();
         }
      }
   }

   // Real vector.

   private void backsub_real () {
            st.l = st.nrun;
            st.H[st.nrun][st.nrun] = 1.0;
            for (int i = st.nrun-1; i >= 0; i--) {
               st.w = st.H[i][i] - st.p;
               st.r = 0.0;
               for (int j = st.l; j <= st.nrun; j++) {
                  st.r = st.r + st.H[i][j] * st.H[j][st.nrun];
               }
               if (st.e[i] < 0.0) {
                  st.z = st.w;
                  st.s = st.r;
               } else {
                  st.l = i;
                  if (st.e[i] == 0.0) {
                     if (st.w != 0.0) {
                        st.H[i][st.nrun] = -st.r / st.w;
                     } else {
                        st.H[i][st.nrun] = -st.r / (st.eps * st.norm);
                     }
   
                  // Solve real equations
   
                  } else {
                     st.x = st.H[i][i+1];
                     st.y = st.H[i+1][i];
                     st.q = (st.d[i] - st.p) * (st.d[i] - st.p) + st.e[i] * st.e[i];
                     st.t = (st.x * st.s - st.z * st.r) / st.q;
                     st.H[i][st.nrun] = st.t;
                     if (Math.abs(st.x) > Math.abs(st.z)) {
                        st.H[i+1][st.nrun] = (-st.r - st.w * st.t) / st.x;
                     } else {
                        st.H[i+1][st.nrun] = (-st.s - st.y * st.t) / st.z;
                     }
                  }
   
                  // Overflow control
   
                  st.t = Math.abs(st.H[i][st.nrun]);
                  if ((st.eps * st.t) * st.t > 1) {
                     for (int j = i; j <= st.nrun; j++) {
                        st.H[j][st.nrun] = st.H[j][st.nrun] / st.t;
                     }
                  }
               }
            }
   }

   // Complex vector.

   private void backsub_complex () {
            st.l = st.nrun-1;

            // Last vector component imaginary so matrix is triangular
   
            if (Math.abs(st.H[st.nrun][st.nrun-1]) > Math.abs(st.H[st.nrun-1][st.nrun])) {
               st.H[st.nrun-1][st.nrun-1] = st.q / st.H[st.nrun][st.nrun-1];
               st.H[st.nrun-1][st.nrun] = -(st.H[st.nrun][st.nrun] - st.p) / st.H[st.nrun][st.nrun-1];
            } else {
               st.cdiv(0.0,-st.H[st.nrun-1][st.nrun],st.H[st.nrun-1][st.nrun-1]-st.p,st.q);
               st.H[st.nrun-1][st.nrun-1] = st.cdivr;
               st.H[st.nrun-1][st.nrun] = st.cdivi;
            }
            st.H[st.nrun][st.nrun-1] = 0.0;
            st.H[st.nrun][st.nrun] = 1.0;
            for (int i = st.nrun-2; i >= 0; i--) {
               st.ra = 0.0;
               st.sa = 0.0;
               for (int j = st.l; j <= st.nrun; j++) {
                  st.ra = st.ra + st.H[i][j] * st.H[j][st.nrun-1];
                  st.sa = st.sa + st.H[i][j] * st.H[j][st.nrun];
               }
               st.w = st.H[i][i] - st.p;
   
               if (st.e[i] < 0.0) {
                  st.z = st.w;
                  st.r = st.ra;
                  st.s = st.sa;
               } else {
                  st.l = i;
                  if (st.e[i] == 0) {
                     st.cdiv(-st.ra,-st.sa,st.w,st.q);
                     st.H[i][st.nrun-1] = st.cdivr;
                     st.H[i][st.nrun] = st.cdivi;
                  } else {
   
                     // Solve complex equations
   
                     st.x = st.H[i][i+1];
                     st.y = st.H[i+1][i];
                     st.vr = (st.d[i] - st.p) * (st.d[i] - st.p) + st.e[i] * st.e[i] - st.q * st.q;
                     st.vi = (st.d[i] - st.p) * 2.0 * st.q;
                     if (st.vr == 0.0 & st.vi == 0.0) {
                        st.vr = st.eps * st.norm * (Math.abs(st.w) + Math.abs(st.q) +
                        Math.abs(st.x) + Math.abs(st.y) + Math.abs(st.z));
                     }
                     st.cdiv(st.x*st.r-st.z*st.ra+st.q*st.sa,st.x*st.s-st.z*st.sa-st.q*st.ra,st.vr,st.vi);
                     st.H[i][st.nrun-1] = st.cdivr;
                     st.H[i][st.nrun] = st.cdivi;
                     if (Math.abs(st.x) > (Math.abs(st.z) + Math.abs(st.q))) {
                        st.H[i+1][st.nrun-1] = (-st.ra - st.w * st.H[i][st.nrun-1] + st.q * st.H[i][st.nrun]) / st.x;
                        st.H[i+1][st.nrun] = (-st.sa - st.w * st.H[i][st.nrun] - st.q * st.H[i][st.nrun-1]) / st.x;
                     } else {
                        st.cdiv(-st.r-st.y*st.H[i][st.nrun-1],-st.s-st.y*st.H[i][st.nrun],st.z,st.q);
                        st.H[i+1][st.nrun-1] = st.cdivr;
                        st.H[i+1][st.nrun] = st.cdivi;
                     }
                  }
   
                  // Overflow control

                  st.t = Math.max(Math.abs(st.H[i][st.nrun-1]),Math.abs(st.H[i][st.nrun]));
                  if ((st.eps * st.t) * st.t > 1) {
                     for (int j = i; j <= st.nrun; j++) {
                        st.H[j][st.nrun-1] = st.H[j][st.nrun-1] / st.t;
                        st.H[j][st.nrun] = st.H[j][st.nrun] / st.t;
                     }
                  }
               }
            }
   }
}
