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
 * InfCodesFast - the fast decode loop of the JZlib 1.1.0 inflate
 * (upstream InfCodes.inflate_fast, verbatim, static; the mask and
 * the Z codes ride the InfCodesC interface) - round-3 reshaping,
 * behavior byte-identical.
 */
final class InfCodesFast implements InfCodesC {
  private InfCodesFast(){}

  static int inflate_fast(int bl, int bd,
                   int[] tl, int tl_index,
                   int[] td, int td_index,
                   InfBlocks s, ZStream z){
    int t;                // temporary pointer
    int[] tp;             // temporary pointer
    int tp_index;         // temporary pointer
    int e;                // extra bits or operation
    int b;                // bit buffer
    int k;                // bits in bit buffer
    int p;                // input data pointer
    int n;                // bytes available there
    int q;                // output window write pointer
    int m;                // bytes to end of window or read pointer
    int ml;               // mask for literal/length tree
    int md;               // mask for distance tree
    int c;                // bytes to copy
    int d;                // distance back to copy from
    int r;                // copy source pointer

    int tp_index_t_3;     // (tp_index+t)*3

    // load input, output, bit values
    p=z.next_in_index;n=z.avail_in;b=s.bitb;k=s.bitk;
    q=s.write;m=q<s.read?s.read-q-1:s.end-q;

    // initialize masks
    ml = inflate_mask[bl];
    md = inflate_mask[bd];

    // do until not enough input or output space for fast loop
    do {                          // assume called with m >= 258 && n >= 10
      // get literal/length code
      while(k<(20)){              // max bits for literal/length code
        n--;
        b|=(z.next_in[p++]&0xff)<<k;k+=8;
      }

      t= b&ml;
      tp=tl; 
      tp_index=tl_index;
      tp_index_t_3=(tp_index+t)*3;
      if ((e = tp[tp_index_t_3]) == 0){
        b>>=(tp[tp_index_t_3+1]); k-=(tp[tp_index_t_3+1]);

        s.window[q++] = (byte)tp[tp_index_t_3+2];
        m--;
        continue;
      }
      do {

        b>>=(tp[tp_index_t_3+1]); k-=(tp[tp_index_t_3+1]);

        if((e&16)!=0){
          e &= 15;
          c = tp[tp_index_t_3+2] + ((int)b & inflate_mask[e]);

          b>>=e; k-=e;

          // decode distance base of block to copy
          while(k<(15)){           // max bits for distance code
            n--;
            b|=(z.next_in[p++]&0xff)<<k;k+=8;
          }

          t= b&md;
          tp=td;
          tp_index=td_index;
          tp_index_t_3=(tp_index+t)*3;
          e = tp[tp_index_t_3];

          do {

            b>>=(tp[tp_index_t_3+1]); k-=(tp[tp_index_t_3+1]);

            if((e&16)!=0){
              // get extra bits to add to distance base
              e &= 15;
              while(k<(e)){         // get extra bits (up to 13)
                n--;
                b|=(z.next_in[p++]&0xff)<<k;k+=8;
              }

              d = tp[tp_index_t_3+2] + (b&inflate_mask[e]);

              b>>=(e); k-=(e);

              // do the copy
              m -= c;
              if (q >= d){                // offset before dest
                //  just copy
                r=q-d;
                if(q-r>0 && 2>(q-r)){           
                  s.window[q++]=s.window[r++]; // minimum count is three,
                  s.window[q++]=s.window[r++]; // so unroll loop a little
                  c-=2;
                }
                else{
                  System.arraycopy(s.window, r, s.window, q, 2);
                  q+=2; r+=2; c-=2;
                }
              }
              else{                  // else offset after destination
                r=q-d;
                do{
                  r+=s.end;          // force pointer in window
                }while(r<0);         // covers invalid distances
                e=s.end-r;
                if(c>e){             // if source crosses,
                  c-=e;              // wrapped copy
                  if(q-r>0 && e>(q-r)){           
                    do{s.window[q++] = s.window[r++];}
                    while(--e!=0);
                  }
                  else{
                    System.arraycopy(s.window, r, s.window, q, e);
                    q+=e; r+=e; e=0;
                  }
                  r = 0;                  // copy rest from start of window
                }

              }

              // copy all or what's left
              if(q-r>0 && c>(q-r)){           
                do{s.window[q++] = s.window[r++];}
                while(--c!=0);
              }
              else{
                System.arraycopy(s.window, r, s.window, q, c);
                q+=c; r+=c; c=0;
              }
              break;
            }
            else if((e&64)==0){
              t+=tp[tp_index_t_3+2];
              t+=(b&inflate_mask[e]);
              tp_index_t_3=(tp_index+t)*3;
              e=tp[tp_index_t_3];
            }
            else{
              z.msg = "invalid distance code";

              c=z.avail_in-n;c=(k>>3)<c?k>>3:c;n+=c;p-=c;k-=c<<3;

              s.bitb=b;s.bitk=k;
              z.avail_in=n;z.total_in+=p-z.next_in_index;z.next_in_index=p;
              s.write=q;

              return Z_DATA_ERROR;
            }
          }
          while(true);
          break;
        }

        if((e&64)==0){
          t+=tp[tp_index_t_3+2];
          t+=(b&inflate_mask[e]);
          tp_index_t_3=(tp_index+t)*3;
          if((e=tp[tp_index_t_3])==0){

            b>>=(tp[tp_index_t_3+1]); k-=(tp[tp_index_t_3+1]);

            s.window[q++]=(byte)tp[tp_index_t_3+2];
            m--;
            break;
          }
        }
        else if((e&32)!=0){

          c=z.avail_in-n;c=(k>>3)<c?k>>3:c;n+=c;p-=c;k-=c<<3;
 
          s.bitb=b;s.bitk=k;
          z.avail_in=n;z.total_in+=p-z.next_in_index;z.next_in_index=p;
          s.write=q;

          return Z_STREAM_END;
        }
        else{
          z.msg="invalid literal/length code";

          c=z.avail_in-n;c=(k>>3)<c?k>>3:c;n+=c;p-=c;k-=c<<3;

          s.bitb=b;s.bitk=k;
          z.avail_in=n;z.total_in+=p-z.next_in_index;z.next_in_index=p;
          s.write=q;

          return Z_DATA_ERROR;
        }
      } 
      while(true);
    } 
    while(m>=258 && n>= 10);

    // not enough input or output--restore pointers and return
    c=z.avail_in-n;c=(k>>3)<c?k>>3:c;n+=c;p-=c;k-=c<<3;

    s.bitb=b;s.bitk=k;
    z.avail_in=n;z.total_in+=p-z.next_in_index;z.next_in_index=p;
    s.write=q;

    return Z_OK;
  }
}
