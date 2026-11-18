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
 * InfBlocksC - the JZlib 1.1.0 InfBlocks constants (upstream
 * verbatim values) hoisted into one constants interface; the
 * InfBlocks family implements it so the state bodies keep the
 * upstream unqualified constant names (round-3 reshaping).
 */
interface InfBlocksC {
  static final int MANY=1440;

  // And'ing with mask[n] masks the lower n bits
  static final int[] inflate_mask = {
    0x00000000, 0x00000001, 0x00000003, 0x00000007, 0x0000000f,
    0x0000001f, 0x0000003f, 0x0000007f, 0x000000ff, 0x000001ff,
    0x000003ff, 0x000007ff, 0x00000fff, 0x00001fff, 0x00003fff,
    0x00007fff, 0x0000ffff
  };

  // Table for deflate from PKZIP's appnote.txt.
  static final int[] border = { // Order of the bit length code lengths
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
  };

  static final int Z_OK=0;
  static final int Z_STREAM_END=1;
  static final int Z_NEED_DICT=2;
  static final int Z_ERRNO=-1;
  static final int Z_STREAM_ERROR=-2;
  static final int Z_DATA_ERROR=-3;
  static final int Z_MEM_ERROR=-4;
  static final int Z_BUF_ERROR=-5;
  static final int Z_VERSION_ERROR=-6;

  static final int TYPE=0;  // get type bits (3, including end bit)
  static final int LENS=1;  // get lengths for stored
  static final int STORED=2;// processing stored block
  static final int TABLE=3; // get table lengths
  static final int BTREE=4; // get bit lengths tree for a dynamic block
  static final int DTREE=5; // get length, distance trees for a dynamic block
  static final int CODES=6; // processing fixed or dynamic block
  static final int DRY=7;   // output remaining window bytes
  static final int DONE=8;  // finished last block, done
  static final int BAD=9;   // ot a data error--stuck here
}
