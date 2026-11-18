package jzlibentry;

/* -*-mode:java; c-basic-offset:2; -*- */
/*
Copyright (c) 2011 ymnk, JCraft,Inc. All rights reserved.

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
 * InfBlocksProc - the inflate BLOCKS state machine (upstream
 * InfBlocks.proc): one instance per InfBlocks owns the cursor
 * locals as fields (the upstream UPDATE-macro sharing model) and
 * each case body is a method; the dispatcher re-dispatches on the
 * mode. Round-3 reshaping, behavior byte-identical.
 */
final class InfBlocksProc implements InfBlocksC {
  private static final int CONT = Integer.MIN_VALUE;
  int t, b, k, p, n, q, m, i, j, c;
  int r;
  InfBlocks s;

  // the upstream inline epilogue: store the cursor back, flush
  private int flush_exit(){
    s.bitb=b; s.bitk=k;
    s.z.avail_in=n;
    s.z.total_in+=p-s.z.next_in_index; s.z.next_in_index=p;
    s.write=q;
    return s.inflate_flush(r);
  }

  private int p_type(){

        while(k<(3)){
          if(n!=0){
            r=Z_OK;
          }
          else{
            return flush_exit();
          };
          n--;
          b|=(s.z.next_in[p++]&0xff)<<k;
          k+=8;
        }
        t = (int)(b & 7);
        s.last = t & 1;

        switch (t >>> 1){
        case 0:                         // stored 
          {b>>>=(3);k-=(3);}
          t = k & 7;                    // go to byte boundary

          {b>>>=(t);k-=(t);}
          s.mode = LENS;                  // get length of stored block
          break;
        case 1:                         // fixed
          InfTree.inflate_trees_fixed(s.bl, s.bd, s.tl, s.td, s.z);
          s.codes.init(s.bl[0], s.bd[0], s.tl[0], 0, s.td[0], 0);

          {b>>>=(3);k-=(3);}

          s.mode = CODES;
          break;
        case 2:                         // dynamic

          {b>>>=(3);k-=(3);}

          s.mode = TABLE;
          break;
        case 3:                         // illegal

          {b>>>=(3);k-=(3);}
          s.mode = BAD;
          s.z.msg = "invalid block type";
          r = Z_DATA_ERROR;

          return flush_exit();
        }
        return CONT;
  }

  private int p_lens(){

        while(k<(32)){
          if(n!=0){
            r=Z_OK;
          }
          else{
            return flush_exit();
          };
          n--;
          b|=(s.z.next_in[p++]&0xff)<<k;
          k+=8;
        }

        if ((((~b) >>> 16) & 0xffff) != (b & 0xffff)){
          s.mode = BAD;
          s.z.msg = "invalid stored block lengths";
          r = Z_DATA_ERROR;

          return flush_exit();
        }
        s.left = (b & 0xffff);
        b = k = 0;                       // dump bits
        s.mode = s.left!=0 ? STORED : (s.last!=0 ? DRY : TYPE);
        return CONT;
  }

  private int p_stored(){
        if (n == 0){
          return flush_exit();
        }

        if(m==0){
          if(q==s.end&&s.read!=0){
            q=0; m=(int)(q<s.read?s.read-q-1:s.end-q);
          }
          if(m==0){
            s.write=q; 
            r=s.inflate_flush(r);
            q=s.write;m=(int)(q<s.read?s.read-q-1:s.end-q);
            if(q==s.end&&s.read!=0){
              q=0; m=(int)(q<s.read?s.read-q-1:s.end-q);
            }
            if(m==0){
              return flush_exit();
            }
          }
        }
        r=Z_OK;

        t = s.left;
        if(t>n) t = n;
        if(t>m) t = m;
        System.arraycopy(s.z.next_in, p, s.window, q, t);
        p += t;  n -= t;
        q += t;  m -= t;
        if ((s.left -= t) != 0)
          return CONT;
        s.mode = s.last!=0 ? DRY : TYPE;
        return CONT;
  }

  private int p_table(){

        while(k<(14)){
          if(n!=0){
            r=Z_OK;
          }
          else{
            return flush_exit();
          };
          n--;
          b|=(s.z.next_in[p++]&0xff)<<k;
          k+=8;
        }

        s.table = t = (b & 0x3fff);
        if ((t & 0x1f) > 29 || ((t >> 5) & 0x1f) > 29)
          {
            s.mode = BAD;
            s.z.msg = "too many length or distance symbols";
            r = Z_DATA_ERROR;

            return flush_exit();
          }
        t = 258 + (t & 0x1f) + ((t >> 5) & 0x1f);
        if(s.blens==null || s.blens.length<t){
          s.blens=new int[t];
        }
        else{
          for(int i=0; i<t; i++){s.blens[i]=0;}
        }

        {b>>>=(14);k-=(14);}

        s.index = 0;
        s.mode = BTREE;
      return CONT;
  }

  private int p_btree(){
        while (s.index < 4 + (s.table >>> 10)){
          while(k<(3)){
            if(n!=0){
              r=Z_OK;
            }
            else{
              return flush_exit();
            };
            n--;
            b|=(s.z.next_in[p++]&0xff)<<k;
            k+=8;
          }

          s.blens[border[s.index++]] = b&7;

          {b>>>=(3);k-=(3);}
        }

        while(s.index < 19){
          s.blens[border[s.index++]] = 0;
        }

        s.bb[0] = 7;
        t = s.inftree.inflate_trees_bits(s.blens, s.bb, s.tb, s.hufts, s.z);
        if (t != Z_OK){
          r = t;
          if (r == Z_DATA_ERROR){
            s.blens=null;
            s.mode = BAD;
          }

          return flush_exit();
        }

        s.index = 0;
        s.mode = DTREE;
      return CONT;
  }

  private int p_codes(){
        s.bitb=b; s.bitk=k;
        s.z.avail_in=n; s.z.total_in+=p-s.z.next_in_index;s.z.next_in_index=p;
        s.write=q;

        if ((r = s.codes.proc(r)) != Z_STREAM_END){
          return flush_exit();
        }
        r = Z_OK;
        s.codes.free(s.z);

        p=s.z.next_in_index; n=s.z.avail_in;b=s.bitb;k=s.bitk;
        q=s.write;m=(int)(q<s.read?s.read-q-1:s.end-q);

        if (s.last==0){
          s.mode = TYPE;
          return CONT;
        }
        s.mode = DRY;
      return CONT;
  }

  private int p_dry(){
        s.write=q; 
        r=s.inflate_flush(r); 
        q=s.write; m=(int)(q<s.read?s.read-q-1:s.end-q);
        if (s.read != s.write){
          return flush_exit();
        }
        s.mode = DONE;
      return CONT;
  }

  private int p_done(){
        r = Z_STREAM_END;

        return flush_exit();
  }

  private int p_bad(){
        r = Z_DATA_ERROR;

        return flush_exit();
  }

  private int p_default(){
        r = Z_STREAM_ERROR;

        return flush_exit();
  }

  // DTREE: the bit-length read loop (upstream while (true) block)
  private int p_dtree(){
        while (true){
          t = s.table;
          if(!(s.index < 258 + (t & 0x1f) + ((t >> 5) & 0x1f))){
            break;
          }

          int[] h;

          t = s.bb[0];

          while(k<(t)){
            if(n!=0){
              r=Z_OK;
            }
            else{
              return flush_exit();
            };
            n--;
            b|=(s.z.next_in[p++]&0xff)<<k;
            k+=8;
          }

          if(s.tb[0]==-1){
            //System.err.println("null...");
          }

          t=s.hufts[(s.tb[0]+(b&inflate_mask[t]))*3+1];
          c=s.hufts[(s.tb[0]+(b&inflate_mask[t]))*3+2];

          if (c < 16){
            b>>>=(t);k-=(t);
            s.blens[s.index++] = c;
          }
        else { // c == 16..18 (the body is p_dtree_repeat)
          int rc = p_dtree_repeat();
          if (rc != CONT){ return rc; }
          }
        }

        s.tb[0]=-1;
        {
          s.bl[0] = 9;         // must be <= 9 for lookahead assumptions
          s.bd[0] = 6;         // must be <= 9 for lookahead assumptions
          t = s.table;
          t = s.inftree.inflate_trees_dynamic(257 + (t & 0x1f), 
                                            1 + ((t >> 5) & 0x1f),
                                            s.blens, s.bl, s.bd, s.tli, s.tdi, s.hufts, s.z);

          if (t != Z_OK){
            if (t == Z_DATA_ERROR){
              s.blens=null;
              s.mode = BAD;
            }
            r = t;

            return flush_exit();
          }
          s.codes.init(s.bl[0], s.bd[0], s.hufts, s.tli[0], s.hufts, s.tdi[0]);
        }
        s.mode = CODES;
      return CONT;
  }

  // DTREE: the c == 16..18 repeat handling (upstream else block)
  private int p_dtree_repeat(){
            i = c == 18 ? 7 : c - 14;
            j = c == 18 ? 11 : 3;

            while(k<(t+i)){
              if(n!=0){
                r=Z_OK;
              }
              else{
                return flush_exit();
              };
              n--;
              b|=(s.z.next_in[p++]&0xff)<<k;
              k+=8;
            }

            b>>>=(t);k-=(t);

            j += (b & inflate_mask[i]);

            b>>>=(i);k-=(i);

            i = s.index;
            t = s.table;
            if (i + j > 258 + (t & 0x1f) + ((t >> 5) & 0x1f) ||
                (c == 16 && i < 1)){
              s.blens=null;
              s.mode = BAD;
              s.z.msg = "invalid bit length repeat";
              r = Z_DATA_ERROR;

              return flush_exit();
            }

            c = c == 16 ? s.blens[i-1] : 0;
            do{
              s.blens[i++] = c;
            }
            while (--j!=0);
            s.index = i;
      return CONT;
  }

  int proc(InfBlocks s, int r){
    this.s = s;
    this.r = r;
    // copy input/output information to locals (UPDATE macro restores)
    p=s.z.next_in_index;n=s.z.avail_in;b=s.bitb;k=s.bitk;
    q=s.write;m=(int)(q<s.read?s.read-q-1:s.end-q);

    // process input based on current state
    while (true){
      int out;
      switch (s.mode){
      case TYPE: out = p_type(); break;
      case LENS: out = p_lens(); break;
      case STORED: out = p_stored(); break;
      case TABLE: out = p_table(); break;
      case BTREE: out = p_btree(); break;
      case CODES: out = p_codes(); break;
      case DRY: out = p_dry(); break;
      case DONE: out = p_done(); break;
      case BAD: out = p_bad(); break;
      default: out = p_default(); break;
      }
      if (out != CONT) return out;
    }
  }
}
