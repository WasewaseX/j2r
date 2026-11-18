package jzlibentry;

/* -*-mode:java; c-basic-offset:2; indent-tabs-mode:nil -*- */
/*
Copyright (c) 2000,2001,2002,2003 ymnk, JCraft,Inc. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

  1. Redistributions of source code must retain the above copyright notice,
     this list of conditions and the following disclaimer.

  2. Redistributions in binary form must reproduce the above copyright 
     notice, this list of conditions and the following disclaimer in 
     the documentation and/or other materials provided with the distribution.

  3. The names of the authors may not be used to endorse or promote products
     derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED ``AS IS'' AND ANY EXPRESSED OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL JCRAFT,
INC. OR ANY CONTRIBUTORS TO THIS SOFTWARE BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA,
OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
/*
 * This program is based on zlib-1.1.3, so all credit should go authors
 * Jean-loup Gailly(jloup@gzip.org) and Mark Adler(madler@alumni.caltech.edu)
 * and contributors of zlib.
/* Round-3 (owner-ordered scanaislop complexity reshaping): a
 * member of the InfBlocks/InfCodes/InfTree family split out of the
 * single-file upstream-merged classes; bodies are the upstream
 * JZlib 1.1.0 bodies with the receiver made explicit - behavior
 * byte-identical, pinned by the javac+java stdout proof and the
 * corpus_suite gate.
 */

/**
 * InfTreeBuild - the JZlib 1.1.0 huft_build table generator with
 * its work area (upstream initWorkArea + work arrays moved here),
 * the 15 work scalars and the huft_build parameters promoted to
 * fields (the instance is per-InfBlocks, the same single-thread
 * sharing model the upstream work arrays had; every field is set
 * at huft_build entry exactly where the upstream local lived).
 * The table-generation loop is split into build_counts /
 * new_tables / make_entry helpers - round-3 reshaping, behavior
 * byte-identical.
 */
final class InfTreeBuild {
  static final int MANY=1440;
  static final int BMAX=15;         // maximum bit length of any code
  static final int Z_OK=0;
  static final int Z_BUF_ERROR=-5;
  static final int Z_DATA_ERROR=-3;

  int[] hn = null;  // hufts used in space
  int[] v = null;   // work area for huft_build 
  int[] c = null;   // bit length count table
  int[] r = null;   // table entry for structure assignment
  int[] u = null;   // table stack
  int[] x = null;   // bit offsets, then code stack

  // the huft_build work scalars, promoted from locals (single
  // work area per InfBlocks; every helper initializes before use)
  int a, f, g, h, i, j, k, l, mask, p, q, w, xp, y, z;

  // the huft_build parameters, promoted so the helpers can read them
  int[] b; int bindex; int n; int s; int[] d; int[] e; int[] t; int[] m; int[] hp;

  void initWorkArea(int vsize){
    if(hn==null){
      hn=new int[1];
      v=new int[vsize];
      c=new int[BMAX+1];
      r=new int[3];
      u=new int[BMAX];
      x=new int[BMAX+1];
    }
    if(v.length<vsize){ v=new int[vsize]; }
    for(int i=0; i<vsize; i++){v[i]=0;}
    for(int i=0; i<BMAX+1; i++){c[i]=0;}
    for(int i=0; i<3; i++){r[i]=0;}
    System.arraycopy(c, 0, u, 0, BMAX);
    System.arraycopy(c, 0, x, 0, BMAX+1);
  }

  // Given a list of code lengths and a maximum table size, make a set of
  // tables to decode that set of codes.  Return Z_OK on success, Z_BUF_ERROR
  // if the given code set is incomplete (the tables are still built in this
  // case), Z_DATA_ERROR if the input is invalid (an over-subscribed set of
  // lengths), or Z_MEM_ERROR if not enough memory.
  int huft_build(int[] b, int bindex, int n, int s, int[] d, int[] e, int[] t, int[] m, int[] hp){
    // the upstream parameter locals, now fields: bind at entry
    this.b=b; this.bindex=bindex; this.n=n; this.s=s;
    this.d=d; this.e=e; this.t=t; this.m=m; this.hp=hp;
    int rc = build_counts();
    if (rc != 0){ return rc; }
    // Make a table of values in order of bit lengths
    i = 0; p = 0;
    do {
      if ((j = b[bindex+p]) != 0){
        v[x[j]++] = i;
      }
      p++;
    }
    while (++i < n);
    n = x[g];                     // set n to length of v

    // Generate the Huffman codes and for each, make the table entries
    x[0] = i = 0;                 // first Huffman code is zero
    p = 0;                        // grab values in bit order
    h = -1;                       // no tables yet--level -1
    w = -l;                       // bits decoded == (l * h)
    u[0] = 0;                     // just to keep compilers happy
    q = 0;                        // ditto
    z = 0;                        // ditto

    // go through the bit lengths (k already is bits in shortest code)
    for (; k <= g; k++){
      a = c[k];
      while (a--!=0){
        // here i is the Huffman code of length k bits for value *p
        // make tables up to required level
        if (new_tables() != 0){ return Z_DATA_ERROR; }
        // set up table entry in r
        make_entry();
        // backup over finished tables
        mask = (1 << w) - 1;      // needed on HP, cc -O bug
        while ((i & mask) != x[h]){
          h--;                    // don't need to update q
          w -= l;
          mask = (1 << w) - 1;
        }
      }
    }
    // Return Z_BUF_ERROR if we were given an incomplete table
    return y != 0 && g != 1 ? Z_BUF_ERROR : Z_OK;
  }

  // the counts phase (upstream 264-312): returns Z_OK for the null
  // input early-out, 0 to continue
  private int build_counts(){
    // Generate counts for each bit length

    p = 0; i = n;
    do {
      c[b[bindex+p]]++; p++; i--;   // assume all entries <= BMAX
    }while(i!=0);

    if(c[0] == n){                // null input--all zero length codes
      t[0] = -1;
      m[0] = 0;
      return Z_OK;
    }

    // Find minimum and maximum length, bound *m by those
    l = m[0];
    for (j = 1; j <= BMAX; j++)
      if(c[j]!=0) break;
    k = j;                        // minimum code length
    if(l < j){
      l = j;
    }
    for (i = BMAX; i!=0; i--){
      if(c[i]!=0) break;
    }
    g = i;                        // maximum code length
    if(l > i){
      l = i;
    }
    m[0] = l;

    // Adjust last length count to fill out codes, if needed
    for (y = 1 << j; j < i; j++, y <<= 1){
      if ((y -= c[j]) < 0){
        return Z_DATA_ERROR;
      }
    }
    if ((y -= c[i]) < 0){
      return Z_DATA_ERROR;
    }
    c[i] += y;

    // Generate starting offsets into the value table for each length
    x[1] = j = 0;
    p = 1;  xp = 2;
    while (--i!=0) {                 // note that i == g from above
      x[xp] = (j += c[p]);
      xp++;
      p++;
    }
    return 0;
  }

  // make tables up to the required level (upstream while (k > w + l)
  // block, operating on the promoted work scalars); returns
  // Z_DATA_ERROR on the MANY overflow (upstream returned straight from
  // huft_build), 0 to continue
  private int new_tables(){
        while (k > w + l){
          h++;
          w += l;                 // previous table always l bits
          // compute minimum size table less than or equal to l bits
          z = g - w;
          z = (z > l) ? l : z;        // table size upper limit
          if((f=1<<(j=k-w))>a+1){     // try a k-w bit table
                                      // too few codes for k-w bit table
            f -= a + 1;               // deduct codes from patterns left
            xp = k;
            if(j < z){
              while (++j < z){        // try smaller tables up to z bits
                if((f <<= 1) <= c[++xp])
                  break;              // enough codes to use up j bits
                f -= c[xp];           // else deduct codes from patterns
              }
            }
          }
          z = 1 << j;                 // table entries for j-bit table

          // allocate new table
          if (hn[0] + z > MANY){       // (note: doesn't matter for fixed)
            return Z_DATA_ERROR;       // overflow of MANY
          }
          u[h] = q = /*hp+*/ hn[0];   // DEBUG
          hn[0] += z;
 
          // connect to last table, if there is one
          if(h!=0){
            x[h]=i;           // save pattern for backing up
            r[0]=(byte)j;     // bits in this table
            r[1]=(byte)l;     // bits to dump before this table
            j=i>>>(w - l);
            r[2] = (int)(q - u[h-1] - j);               // offset to this table
            System.arraycopy(r, 0, hp, (u[h-1]+j)*3, 3); // connect to last table
          }
          else{
            t[0] = q;               // first table is returned result
          }
        }
    return 0;
  }

  // set up one table entry in r, fill the code-like entries and
  // backwards-increment the k-bit code i (upstream block)
  private void make_entry(){
        // set up table entry in r
        r[1] = (byte)(k - w);
        if (p >= n){
          r[0] = 128 + 64;      // out of values--invalid code
        }
        else if (v[p] < s){
          r[0] = (byte)(v[p] < 256 ? 0 : 32 + 64);  // 256 is end-of-block
          r[2] = v[p++];          // simple code is just the value
        }
        else{
          r[0]=(byte)(e[v[p]-s]+16+64); // non-simple--look up in lists
          r[2]=d[v[p++] - s];
        }

        // fill code-like entries with r
        f=1<<(k-w);
        for (j=i>>>w;j<z;j+=f){
          System.arraycopy(r, 0, hp, (q+j)*3, 3);
        }

        // backwards increment the k-bit code i
        for (j = 1 << (k - 1); (i & j)!=0; j >>>= 1){
          i ^= j;
        }
        i ^= j;
  }
}
