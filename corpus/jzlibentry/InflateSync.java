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


/* Round-3 (owner-ordered scanaislop complexity reshaping): this file
 * is a member of the Inflate family split out of the single 753-line
 * Inflate.java; bodies are the upstream JZlib 1.1.0 bodies with the
 * Inflate receiver made explicit (static, `s` param) and the state
 * returns expressed through the shared Return control-flow exception
 * - behavior byte-identical, pinned by the javac+java stdout proof
 * and the corpus_suite gate.
 */

/**
 * InflateSync - the dictionary-set and sync/restart surface of the
 * JZlib 1.1.0 inflate (upstream bodies, the Inflate receiver made
 * explicit; round-3 reshaping, behavior byte-identical).
 */
final class InflateSync implements InflateC {
  private InflateSync(){}

  static int inflateSetDictionary(Inflate s, byte[] dictionary, int dictLength){
    if(s.z==null || (s.mode != DICT0 && s.wrap != 0)){
      return Z_STREAM_ERROR;
    }

    int index=0;
    int length = dictLength;

    if(s.mode==DICT0){
      long adler_need=s.z.adler.getValue();
      s.z.adler.reset();
      s.z.adler.update(dictionary, 0, dictLength);
      if(s.z.adler.getValue()!=adler_need){
        return Z_DATA_ERROR;
      }
    }

    s.z.adler.reset();

    if(length >= (1<<s.wbits)){
      length = (1<<s.wbits)-1;
      index=dictLength - length;
    }
    s.blocks.set_dictionary(dictionary, index, length);
    s.mode = BLOCKS;
    return Z_OK;
  }

  static final byte[] mark = {(byte)0, (byte)0, (byte)0xff, (byte)0xff};

  static int inflateSync(Inflate s){
    int n;       // number of bytes to look at
    int p;       // pointer to bytes
    int m;       // number of marker bytes found in a row
    long r, w;   // temporaries to save total_in and total_out

    // set up
    if(s.z == null)
      return Z_STREAM_ERROR;
    if(s.mode != BAD){
      s.mode = BAD;
      s.marker = 0;
    }
    if((n=s.z.avail_in)==0)
      return Z_BUF_ERROR;

    p=s.z.next_in_index;
    m=s.marker;
    // search
    while (n!=0 && m < 4){
      if(s.z.next_in[p] == mark[m]){
        m++;
      }
      else if(s.z.next_in[p]!=0){
        m = 0;
      }
      else{
        m = 4 - m;
      }
      p++; n--;
    }

    // restore
    s.z.total_in += p-s.z.next_in_index;
    s.z.next_in_index = p;
    s.z.avail_in = n;
    s.marker = m;

    // return no joy or set up to restart on a new block
    if(m != 4){
      return Z_DATA_ERROR;
    }
    r=s.z.total_in;  w=s.z.total_out;
    s.inflateReset();
    s.z.total_in=r;  s.z.total_out = w;
    s.mode = BLOCKS;

    return Z_OK;
  }

  static int inflateSyncPoint(Inflate s){
    if(s.z == null || s.blocks == null)
      return Z_STREAM_ERROR;
    return s.blocks.sync_point();
  }
}
