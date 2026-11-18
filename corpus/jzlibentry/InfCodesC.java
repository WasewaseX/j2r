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
 * InfCodesC - the JZlib 1.1.0 InfCodes constants (upstream
 * verbatim values) hoisted into one constants interface; the
 * InfCodes family implements it so the state bodies keep the
 * upstream unqualified constant names (round-3 reshaping).
 */
interface InfCodesC {
  static final int[] inflate_mask = {
    0x00000000, 0x00000001, 0x00000003, 0x00000007, 0x0000000f,
    0x0000001f, 0x0000003f, 0x0000007f, 0x000000ff, 0x000001ff,
    0x000003ff, 0x000007ff, 0x00000fff, 0x00001fff, 0x00003fff,
    0x00007fff, 0x0000ffff
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

  // waiting for "i:"=input,
  //             "o:"=output,
  //             "x:"=nothing
  static final int START=0;  // x: set up for LEN
  static final int LEN=1;    // i: get length/literal/eob next
  static final int LENEXT=2; // i: getting length extra (have base)
  static final int DIST=3;   // i: get distance next
  static final int DISTEXT=4;// i: getting distance extra
  static final int COPY=5;   // o: copying bytes in window, waiting for space
  static final int LIT=6;    // o: got literal, waiting for output space
  static final int WASH=7;   // o: got eob, possibly still output waiting
  static final int END=8;    // x: got eob and all data flushed
  static final int BADCODE=9;// x: got error
}
