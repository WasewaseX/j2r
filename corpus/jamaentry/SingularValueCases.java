package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: SingularValueCases holds the four kase bodies of the
 * Golub-Reinsch main iteration (deflate / split / qr step /
 * convergence), split from the single-file SingularValueDecomposition
 * source per the owner's scanaislop complexity order. It reaches the
 * SingularValueCore work area through the st receiver; the case-local
 * scratch (f/t/cs/sn/g and the rotation parameters) stays local
 * exactly as upstream. Hand-maintained after this generation step.
 */

final class SingularValueCases {

   private SingularValueCases() {}

   // kase 1: deflate negligible s(p).

   static void case1(SingularValueCore st) {
               double f = st.e[st.p-2];
               st.e[st.p-2] = 0.0;
               for (int j = st.p-2; j >= st.k; j--) {
                  double t = Maths.hypot(st.s[j],f);
                  double cs = st.s[j]/t;
                  double sn = f/t;
                  st.s[j] = t;
                  if (j != st.k) {
                     f = -sn*st.e[j-1];
                     st.e[j-1] = cs*st.e[j-1];
                  }
                  if (st.wantv) {
                     for (int i = 0; i < st.n; i++) {
                        t = cs*st.V[i][j] + sn*st.V[i][st.p-1];
                        st.V[i][st.p-1] = -sn*st.V[i][j] + cs*st.V[i][st.p-1];
                        st.V[i][j] = t;
                     }
                  }
               }
   }

   // kase 2: split at negligible s(k).

   static void case2(SingularValueCore st) {
               double f = st.e[st.k-1];
               st.e[st.k-1] = 0.0;
               for (int j = st.k; j < st.p; j++) {
                  double t = Maths.hypot(st.s[j],f);
                  double cs = st.s[j]/t;
                  double sn = f/t;
                  st.s[j] = t;
                  f = -sn*st.e[j];
                  st.e[j] = cs*st.e[j];
                  if (st.wantu) {
                     for (int i = 0; i < st.m; i++) {
                        t = cs*st.U[i][j] + sn*st.U[i][st.k-1];
                        st.U[i][st.k-1] = -sn*st.U[i][j] + cs*st.U[i][st.k-1];
                        st.U[i][j] = t;
                     }
                  }
               }
   }

   // kase 3: perform one qr step.

   static void case3(SingularValueCore st) {
               // Calculate the shift.
   
               double scale = Math.max(Math.max(Math.max(Math.max(
                       Math.abs(st.s[st.p-1]),Math.abs(st.s[st.p-2])),Math.abs(st.e[st.p-2])), 
                       Math.abs(st.s[st.k])),Math.abs(st.e[st.k]));
               double sp = st.s[st.p-1]/scale;
               double spm1 = st.s[st.p-2]/scale;
               double epm1 = st.e[st.p-2]/scale;
               double sk = st.s[st.k]/scale;
               double ek = st.e[st.k]/scale;
               double b = ((spm1 + sp)*(spm1 - sp) + epm1*epm1)/2.0;
               double c = (sp*epm1)*(sp*epm1);
               double shift = 0.0;
               if ((b != 0.0) | (c != 0.0)) {
                  shift = Math.sqrt(b*b + c);
                  if (b < 0.0) {
                     shift = -shift;
                  }
                  shift = c/(b + shift);
               }
               double f = (sk + sp)*(sk - sp) + shift;
               double g = sk*ek;
   
               // Chase zeros.
   
               for (int j = st.k; j < st.p-1; j++) {
                  double t = Maths.hypot(f,g);
                  double cs = f/t;
                  double sn = g/t;
                  if (j != st.k) {
                     st.e[j-1] = t;
                  }
                  f = cs*st.s[j] + sn*st.e[j];
                  st.e[j] = cs*st.e[j] - sn*st.s[j];
                  g = sn*st.s[j+1];
                  st.s[j+1] = cs*st.s[j+1];
                  if (st.wantv) {
                     for (int i = 0; i < st.n; i++) {
                        t = cs*st.V[i][j] + sn*st.V[i][j+1];
                        st.V[i][j+1] = -sn*st.V[i][j] + cs*st.V[i][j+1];
                        st.V[i][j] = t;
                     }
                  }
                  t = Maths.hypot(f,g);
                  cs = f/t;
                  sn = g/t;
                  st.s[j] = t;
                  f = cs*st.e[j] + sn*st.s[j+1];
                  st.s[j+1] = -sn*st.e[j] + cs*st.s[j+1];
                  g = sn*st.e[j+1];
                  st.e[j+1] = cs*st.e[j+1];
                  if (st.wantu && (j < st.m-1)) {
                     for (int i = 0; i < st.m; i++) {
                        t = cs*st.U[i][j] + sn*st.U[i][j+1];
                        st.U[i][j+1] = -sn*st.U[i][j] + cs*st.U[i][j+1];
                        st.U[i][j] = t;
                     }
                  }
               }
               st.e[st.p-2] = f;
               st.iter = st.iter + 1;
   }

   // kase 4: convergence (make the singular values positive, order
   // them, retire p).

   static void case4(SingularValueCore st) {
               // Make the singular values positive.
   
               if (st.s[st.k] <= 0.0) {
                  st.s[st.k] = (st.s[st.k] < 0.0 ? -st.s[st.k] : 0.0);
                  if (st.wantv) {
                     for (int i = 0; i <= st.pp; i++) {
                        st.V[i][st.k] = -st.V[i][st.k];
                     }
                  }
               }
   
               // Order the singular values.
   
               while (st.k < st.pp) {
                  if (st.s[st.k] >= st.s[st.k+1]) {
                     break;
                  }
                  double t = st.s[st.k];
                  st.s[st.k] = st.s[st.k+1];
                  st.s[st.k+1] = t;
                  if (st.wantv && (st.k < st.n-1)) {
                     for (int i = 0; i < st.n; i++) {
                        t = st.V[i][st.k+1]; st.V[i][st.k+1] = st.V[i][st.k]; st.V[i][st.k] = t;
                     }
                  }
                  if (st.wantu && (st.k < st.m-1)) {
                     for (int i = 0; i < st.m; i++) {
                        t = st.U[i][st.k+1]; st.U[i][st.k+1] = st.U[i][st.k]; st.U[i][st.k] = t;
                     }
                  }
                  st.k++;
               }
               st.iter = 0;
               st.p--;
   }
}
