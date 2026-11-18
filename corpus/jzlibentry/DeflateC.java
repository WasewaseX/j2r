package jzlibentry;

/* -*-mode:java; c-basic-offset:2; -*- */
/*
Copyright (c) 2000-2011 ymnk, JCraft,Inc. All rights reserved.

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
 */

/**
 * DeflateC - the JZlib 1.1.0 Deflate constants (upstream verbatim
 * values) hoisted into one constants interface. The Deflate family
 * implements it so the algorithm bodies keep the upstream
 * unqualified constant names (owner-ordered round-3 reshaping;
 * behavior pinned by the byte-identical javac+java stdout proof
 * and the corpus_suite gate).
 */
interface DeflateC {
  static final int MAX_MEM_LEVEL=9;

  static final int Z_DEFAULT_COMPRESSION=-1;

  static final int MAX_WBITS=15;            // 32K LZ77 window
  static final int DEF_MEM_LEVEL=8;

  static final int STORED=0;
  static final int FAST=1;
  static final int SLOW=2;
  static final Config[] config_table = Config.build_table();
  static class Config{
    int good_length; // reduce lazy search above this match length
    int max_lazy;    // do not perform lazy search above this match length
    int nice_length; // quit search above this match length
    int max_chain;
    int func;
    Config(int good_length, int max_lazy, 
           int nice_length, int max_chain, int func){
      this.good_length=good_length;
      this.max_lazy=max_lazy;
      this.nice_length=nice_length;
      this.max_chain=max_chain;
      this.func=func;
    }
    static Config[] build_table(){
      Config[] t = new Config[10];
    t=new Config[10];
    //                         good  lazy  nice  chain
    t[0]=new Config(0,    0,    0,    0, STORED);
    t[1]=new Config(4,    4,    8,    4, FAST);
    t[2]=new Config(4,    5,   16,    8, FAST);
    t[3]=new Config(4,    6,   32,   32, FAST);
    
    t[4]=new Config(4,    4,   16,   16, SLOW);
    t[5]=new Config(8,   16,   32,   32, SLOW);
    t[6]=new Config(8,   16,  128,  128, SLOW);
    t[7]=new Config(8,   32,  128,  256, SLOW);
    t[8]=new Config(32, 128,  258, 1024, SLOW);
    t[9]=new Config(32, 258,  258, 4096, SLOW);
      return t;
    }
  }
  static final String[] z_errmsg = {
    "need dictionary",     // Z_NEED_DICT       2
    "stream end",          // Z_STREAM_END      1
    "",                    // Z_OK              0
    "file error",          // Z_ERRNO         (-1)
    "stream error",        // Z_STREAM_ERROR  (-2)
    "data error",          // Z_DATA_ERROR    (-3)
    "insufficient memory", // Z_MEM_ERROR     (-4)
    "buffer error",        // Z_BUF_ERROR     (-5)
    "incompatible version",// Z_VERSION_ERROR (-6)
    ""
  };

  // block not completed, need more input or more output
  static final int NeedMore=0; 

  // block flush performed
  static final int BlockDone=1; 

  // finish started, need only more output at next deflate
  static final int FinishStarted=2;

  // finish done, accept no more input or output
  static final int FinishDone=3;

  // preset dictionary flag in zlib header
  static final int PRESET_DICT=0x20;

  static final int Z_FILTERED=1;
  static final int Z_HUFFMAN_ONLY=2;
  static final int Z_DEFAULT_STRATEGY=0;

  static final int Z_NO_FLUSH=0;
  static final int Z_PARTIAL_FLUSH=1;
  static final int Z_SYNC_FLUSH=2;
  static final int Z_FULL_FLUSH=3;
  static final int Z_FINISH=4;

  static final int Z_OK=0;
  static final int Z_STREAM_END=1;
  static final int Z_NEED_DICT=2;
  static final int Z_ERRNO=-1;
  static final int Z_STREAM_ERROR=-2;
  static final int Z_DATA_ERROR=-3;
  static final int Z_MEM_ERROR=-4;
  static final int Z_BUF_ERROR=-5;
  static final int Z_VERSION_ERROR=-6;

  static final int INIT_STATE=42;
  static final int BUSY_STATE=113;
  static final int FINISH_STATE=666;

  // The deflate compression method
  static final int Z_DEFLATED=8;

  static final int STORED_BLOCK=0;
  static final int STATIC_TREES=1;
  static final int DYN_TREES=2;

  // The three kinds of block type
  static final int Z_BINARY=0;
  static final int Z_ASCII=1;
  static final int Z_UNKNOWN=2;

  static final int Buf_size=8*2;

  // repeat previous bit length 3-6 times (2 bits of repeat count)
  static final int REP_3_6=16; 

  // repeat a zero length 3-10 times  (3 bits of repeat count)
  static final int REPZ_3_10=17; 

  // repeat a zero length 11-138 times  (7 bits of repeat count)
  static final int REPZ_11_138=18; 

  static final int MIN_MATCH=3;
  static final int MAX_MATCH=258;
  static final int MIN_LOOKAHEAD=(MAX_MATCH+MIN_MATCH+1);

  static final int MAX_BITS=15;
  static final int D_CODES=30;
  static final int BL_CODES=19;
  static final int LENGTH_CODES=29;
  static final int LITERALS=256;
  static final int L_CODES=(LITERALS+1+LENGTH_CODES);
  static final int HEAP_SIZE=(2*L_CODES+1);

  static final int END_BLOCK=256;
}
