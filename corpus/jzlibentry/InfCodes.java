package jzlibentry;

/* -*-mode:java; c-basic-offset:2; -*- */
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
 * The JZlib 1.1.0 InfCodes state machine: the proc() dispatcher
 * (upstream case-switch, each case body now a method re-dispatched
 * on the mode) with the cursor locals promoted to fields (one
 * InfCodes per InfBlocks, the upstream single-thread sharing
 * model) - round-3 reshaping, behavior byte-identical.
 */
final class InfCodes implements InfCodesC {
  private static final int CONT = Integer.MIN_VALUE;

  // the proc cursor locals, promoted from upstream locals
  int j; int[] t; int tindex; int e; int f;
  int b, k, p, n, q, m;
  int r;

  int mode;      // current inflate_codes mode

  // mode dependent information
  int len;

  int[] tree; // pointer into tree
  int tree_index=0;
  int need;   // bits needed

  int lit;

  // if EXT or COPY, where and how much
  int get;              // bits to get for extra
  int dist;             // distance back to copy from

  byte lbits;           // ltree bits decoded per branch
  byte dbits;           // dtree bits decoder per branch
  int[] ltree;          // literal/length/eob tree
  int ltree_index;      // literal/length/eob tree
  int[] dtree;          // distance tree
  int dtree_index;      // distance tree

  private final ZStream z;
  private final InfBlocks s;
  InfCodes(ZStream z, InfBlocks s){
    this.z=z; 
    this.s=s; 
  }

  void init(int bl, int bd,
           int[] tl, int tl_index,
           int[] td, int td_index){
    mode=START;
    lbits=(byte)bl;
    dbits=(byte)bd;
    ltree=tl;
    ltree_index=tl_index;
    dtree = td;
    dtree_index=td_index;
    tree=null;
  }

  // the upstream inline epilogue: store the cursor back, flush
  private int flush_exit(){
    s.bitb=b;s.bitk=k;
    z.avail_in=n;z.total_in+=p-z.next_in_index;z.next_in_index=p;
    s.write=q;
    return s.inflate_flush(r);
  }

  private int p_start(){
        if (m >= 258 && n >= 10){

          s.bitb=b;s.bitk=k;
          z.avail_in=n;z.total_in+=p-z.next_in_index;z.next_in_index=p;
          s.write=q;
          r = InfCodesFast.inflate_fast(lbits, dbits, 
                           ltree, ltree_index, 
                           dtree, dtree_index,
                           s, z);

          p=z.next_in_index;n=z.avail_in;b=s.bitb;k=s.bitk;
          q=s.write;m=q<s.read?s.read-q-1:s.end-q;

          if (r != Z_OK){
            mode = r == Z_STREAM_END ? WASH : BADCODE;
            return CONT;
          }
        }
        need = lbits;
        tree = ltree;
        tree_index=ltree_index;

        mode = LEN;
      return CONT;
  }

  private int p_len(){
        j = need;

        while(k<(j)){
          if(n!=0)r=Z_OK;
          else{

            return flush_exit();
          }
          n--;
          b|=(z.next_in[p++]&0xff)<<k;
          k+=8;
        }

        tindex=(tree_index+(b&inflate_mask[j]))*3;

        b>>>=(tree[tindex+1]);
        k-=(tree[tindex+1]);

        e=tree[tindex];

        if(e == 0){               // literal
          lit = tree[tindex+2];
          mode = LIT;
          return CONT;
        }
        if((e & 16)!=0 ){          // length
          get = e & 15;
          len = tree[tindex+2];
          mode = LENEXT;
          return CONT;
        }
        if ((e & 64) == 0){        // next table
          need = e;
          tree_index = tindex/3+tree[tindex+2];
          return CONT;
        }
        if ((e & 32)!=0){               // end of block
          mode = WASH;
          return CONT;
        }
        mode = BADCODE;        // invalid code
        z.msg = "invalid literal/length code";
        r = Z_DATA_ERROR;

        return flush_exit();
  }

  private int p_lenext(){
        j = get;

        while(k<(j)){
          if(n!=0)r=Z_OK;
          else{

            return flush_exit();
          }
          n--; b|=(z.next_in[p++]&0xff)<<k;
          k+=8;
        }

        len += (b & inflate_mask[j]);

        b>>=j;
        k-=j;

        need = dbits;
        tree = dtree;
        tree_index=dtree_index;
        mode = DIST;
      return CONT;
  }

  private int p_dist(){
        j = need;

        while(k<(j)){
          if(n!=0)r=Z_OK;
          else{

            return flush_exit();
          }
          n--; b|=(z.next_in[p++]&0xff)<<k;
          k+=8;
        }

        tindex=(tree_index+(b & inflate_mask[j]))*3;

        b>>=tree[tindex+1];
        k-=tree[tindex+1];

        e = (tree[tindex]);
        if((e & 16)!=0){               // distance
          get = e & 15;
          dist = tree[tindex+2];
          mode = DISTEXT;
          return CONT;
        }
        if ((e & 64) == 0){        // next table
          need = e;
          tree_index = tindex/3 + tree[tindex+2];
          return CONT;
        }
        mode = BADCODE;        // invalid code
        z.msg = "invalid distance code";
        r = Z_DATA_ERROR;

        return flush_exit();
  }

  private int p_distext(){
        j = get;

        while(k<(j)){
          if(n!=0)r=Z_OK;
          else{

            return flush_exit();
          }
          n--; b|=(z.next_in[p++]&0xff)<<k;
          k+=8;
        }

        dist += (b & inflate_mask[j]);

        b>>=j;
        k-=j;

        mode = COPY;
      return CONT;
  }

  private int p_copy(){
        f = q - dist;
        while(f < 0){     // modulo window size-"while" instead
          f += s.end;     // of "if" handles invalid distances
        }
        while (len!=0){

          if(m==0){
            if(q==s.end&&s.read!=0){q=0;m=q<s.read?s.read-q-1:s.end-q;}
            if(m==0){
              s.write=q; r=s.inflate_flush(r);
              q=s.write;m=q<s.read?s.read-q-1:s.end-q;

              if(q==s.end&&s.read!=0){q=0;m=q<s.read?s.read-q-1:s.end-q;}

              if(m==0){
                return flush_exit();
              }  
            }
          }

          s.window[q++]=s.window[f++]; m--;

          if (f == s.end)
            f = 0;
          len--;
        }
        mode = START;
        return CONT;
  }

  private int p_lit(){
        if(m==0){
          if(q==s.end&&s.read!=0){q=0;m=q<s.read?s.read-q-1:s.end-q;}
          if(m==0){
            s.write=q; r=s.inflate_flush(r);
            q=s.write;m=q<s.read?s.read-q-1:s.end-q;

            if(q==s.end&&s.read!=0){q=0;m=q<s.read?s.read-q-1:s.end-q;}
            if(m==0){
              return flush_exit();
            }
          }
        }
        r=Z_OK;

        s.window[q++]=(byte)lit; m--;

        mode = START;
        return CONT;
  }

  private int p_wash(){
        if (k > 7){        // return unused byte, if any
          k -= 8;
          n++;
          p--;             // can always return one
        }

        s.write=q; r=s.inflate_flush(r);
        q=s.write;m=q<s.read?s.read-q-1:s.end-q;

        if (s.read != s.write){
          return flush_exit();
        }
        mode = END;
      return CONT;
  }

  private int p_end(){
        r = Z_STREAM_END;
        return flush_exit();
  }

  private int p_badcode(){

        r = Z_DATA_ERROR;

        return flush_exit();
  }

  private int p_default(){
        r = Z_STREAM_ERROR;

        return flush_exit();
  }

  int proc(int rr){
    r = rr;
    // copy input/output information to locals (UPDATE macro restores)
    p=s.z.next_in_index;n=s.z.avail_in;b=s.bitb;k=s.bitk;
    q=s.write;m=q<s.read?s.read-q-1:s.end-q;

    // process input and output based on current state
    while (true){
      int out;
      switch (mode){
      case START: out = p_start(); break;
      case LEN: out = p_len(); break;
      case LENEXT: out = p_lenext(); break;
      case DIST: out = p_dist(); break;
      case DISTEXT: out = p_distext(); break;
      case COPY: out = p_copy(); break;
      case LIT: out = p_lit(); break;
      case WASH: out = p_wash(); break;
      case END: out = p_end(); break;
      case BADCODE: out = p_badcode(); break;
      default: out = p_default(); break;
      }
      if (out != CONT) return out;
    }
  }

  void free(ZStream z){
    //  ZFREE(z, c);
  }
}
