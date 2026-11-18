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
 * InflateC - the JZlib 1.1.0 inflate constants (upstream verbatim
 * values) hoisted into one constants interface. The Inflate family
 * implements it so the state bodies keep the upstream unqualified
 * constant names (round-3 reshaping; behavior byte-identical).
 */
interface InflateC {
  static final int MAX_WBITS=15; // 32K LZ77 window

  // preset dictionary flag in zlib header
  static final int PRESET_DICT=0x20;

  static final int Z_NO_FLUSH=0;
  static final int Z_PARTIAL_FLUSH=1;
  static final int Z_SYNC_FLUSH=2;
  static final int Z_FULL_FLUSH=3;
  static final int Z_FINISH=4;

  static final int Z_DEFLATED=8;

  static final int Z_OK=0;
  static final int Z_STREAM_END=1;
  static final int Z_NEED_DICT=2;
  static final int Z_ERRNO=-1;
  static final int Z_STREAM_ERROR=-2;
  static final int Z_DATA_ERROR=-3;
  static final int Z_MEM_ERROR=-4;
  static final int Z_BUF_ERROR=-5;
  static final int Z_VERSION_ERROR=-6;

  static final int METHOD=0;   // waiting for method byte
  static final int FLAG=1;     // waiting for flag byte
  static final int DICT4=2;    // four dictionary check bytes to go
  static final int DICT3=3;    // three dictionary check bytes to go
  static final int DICT2=4;    // two dictionary check bytes to go
  static final int DICT1=5;    // one dictionary check byte to go
  static final int DICT0=6;    // waiting for inflateSetDictionary
  static final int BLOCKS=7;   // decompressing blocks
  static final int CHECK4=8;   // four check bytes to go
  static final int CHECK3=9;   // three check bytes to go
  static final int CHECK2=10;  // two check bytes to go
  static final int CHECK1=11;  // one check byte to go
  static final int DONE=12;    // finished check, done
  static final int BAD=13;     // got an error--stay here

  static final int HEAD=14;
  static final int LENGTH=15;
  static final int TIME=16;
  static final int OS=17;
  static final int EXLEN=18;
  static final int EXTRA=19;
  static final int NAME=20;
  static final int COMMENT=21;
  static final int HCRC=22;
  static final int FLAGS=23;

  static final int INFLATE_ANY=0x40000000;
}
