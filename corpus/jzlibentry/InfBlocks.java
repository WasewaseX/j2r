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

final class InfBlocks implements InfBlocksC {
  int mode;            // current inflate_block mode 

  int left;            // if STORED, bytes left to copy 

  int table;           // table lengths (14 bits) 
  int index;           // index into blens (or border) 
  int[] blens;         // bit lengths of codes 
  int[] bb=new int[1]; // bit length tree depth 
  int[] tb=new int[1]; // bit length decoding tree 

  int[] bl=new int[1];
  int[] bd=new int[1];

  int[][] tl=new int[1][];
  int[][] td=new int[1][];
  int[] tli=new int[1]; // tl_index
  int[] tdi=new int[1]; // td_index

  final InfCodes codes;      // if CODES, current state 

  int last;            // true if this block is the last block 

  // mode independent information 
  int bitk;            // bits in bit buffer 
  int bitb;            // bit buffer 
  int[] hufts;         // single malloc for tree space 
  byte[] window;       // sliding window 
  int end;             // one byte after sliding window 
  int read;            // window read pointer 
  int write;           // window write pointer 
  private boolean check;
  final InfTree inftree=new InfTree();

  final ZStream z;
  final InfBlocksProc procState = new InfBlocksProc();

  InfBlocks(ZStream z, int w){
    this.z=z;
    this.codes=new InfCodes(this.z, this);
    hufts=new int[MANY*3];
    window=new byte[w];
    end=w;
    this.check = (z.istate.wrap==0) ? false : true;
    mode = TYPE;
    reset();
  }

  void reset(){
    if(mode==BTREE || mode==DTREE){
    }
    if(mode==CODES){
      codes.free(z);
    }
    mode=TYPE;
    bitk=0;
    bitb=0;
    read=write=0;
    if(check){
      z.adler.reset();
    }
  }

  int proc(int r){ return procState.proc(this, r); }

  void free(){
    reset();
    window=null;
    hufts=null;
    //ZFREE(z, s);
  }

  void set_dictionary(byte[] d, int start, int n){
    System.arraycopy(d, start, window, 0, n);
    read = write = n;
  }

  // Returns true if inflate is currently at the end of a block generated
  // by Z_SYNC_FLUSH or Z_FULL_FLUSH. 
  int sync_point(){
    return mode == LENS ? 1 : 0;
  }

  // copy as much as possible from the sliding window to the output area
  int inflate_flush(int r){
    int n;
    int p;
    int q;

    // local copies of source and destination pointers
    p = z.next_out_index;
    q = read;

    n = (int)((q <= write ? write : end) - q);
    if(n > z.avail_out) n = z.avail_out;
    if(n!=0 && r == Z_BUF_ERROR) r = Z_OK;

    z.avail_out -= n;
    z.total_out += n;

    if(check && n>0){
      z.adler.update(window, q, n);
    }

    System.arraycopy(window, q, z.next_out, p, n);
    p += n;
    q += n;

    if (q == end){
      q = 0;
      if (write == end)
        write = 0;

      n = write - q;
      if (n > z.avail_out) n = z.avail_out;
      if (n!=0 && r == Z_BUF_ERROR) r = Z_OK;

      z.avail_out -= n;
      z.total_out += n;

      if(check && n>0){
        z.adler.update(window, q, n);
      }

      // copy
      System.arraycopy(window, q, z.next_out, p, n);
      p += n;
      q += n;
    }

    // update pointers
    z.next_out_index = p;
    read = q;

    // done
    return r;
  }
}
