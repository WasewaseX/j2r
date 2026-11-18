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

/* Round-3 (owner-ordered scanaislop complexity reshaping): this
 * file is a member of the Deflate family split out of the single
 * 1761-line upstream-merged Deflate.java. Method bodies are the
 * upstream JZlib 1.1.0 bodies with the Deflate receiver made
 * explicit (static, `s` param) - behavior byte-identical, pinned
 * by the javac+java stdout proof and the corpus_suite gate.
 */

/**
 * DeflateBit - the pending-output/bit-buffer group of the JZlib
 * 1.1.0 Deflate (upstream bodies, the Deflate receiver made
 * explicit; round-3 reshaping, behavior byte-identical).
 */
final class DeflateBit implements DeflateC {
  private DeflateBit(){}

  static void put_byte(Deflate s, byte[] p, int start, int len){
    System.arraycopy(p, start, s.pending_buf, s.pending, len);
    s.pending+=len;
  }

  static void put_byte(Deflate s, byte c){
    s.pending_buf[s.pending++]=c;
  }

  static void put_short(Deflate s, int w) {
    s.put_byte((byte)(w/*&0xff*/));
    s.put_byte((byte)(w>>>8));
  }

  static void putShortMSB(Deflate s, int b){
    s.put_byte((byte)(b>>8));
    s.put_byte((byte)(b/*&0xff*/));
  }

  static void send_code(Deflate s, int c, short[] tree){
    int c2=c*2;
    s.send_bits((tree[c2]&0xffff), (tree[c2+1]&0xffff));
  }

  static void send_bits(Deflate s, int value, int length){
    int len = length;
    if (s.bi_valid > (int)Buf_size - len) {
      int val = value;
//      bi_buf |= (val << bi_valid);
      s.bi_buf |= ((val << s.bi_valid)&0xffff);
      s.put_short(s.bi_buf);
      s.bi_buf = (short)(val >>> (Buf_size - s.bi_valid));
      s.bi_valid += len - Buf_size;
    } else {
//      bi_buf |= (value) << bi_valid;
      s.bi_buf |= (((value) << s.bi_valid)&0xffff);
      s.bi_valid += len;
    }
  }

  static void bi_flush(Deflate s){
    if (s.bi_valid == 16) {
      s.put_short(s.bi_buf);
      s.bi_buf=0;
      s.bi_valid=0;
    }
    else if (s.bi_valid >= 8) {
      s.put_byte((byte)s.bi_buf);
      s.bi_buf>>>=8;
      s.bi_valid-=8;
    }
  }

  static void bi_windup(Deflate s){
    if (s.bi_valid > 8) {
      s.put_short(s.bi_buf);
    } else if (s.bi_valid > 0) {
      s.put_byte((byte)s.bi_buf);
    }
    s.bi_buf = 0;
    s.bi_valid = 0;
  }

  static void set_data_type(Deflate s){
    int n = 0;
    int  ascii_freq = 0;
    int  bin_freq = 0;
    while(n<7){ bin_freq += s.dyn_ltree[n*2]; n++;}
    while(n<128){ ascii_freq += s.dyn_ltree[n*2]; n++;}
    while(n<LITERALS){ bin_freq += s.dyn_ltree[n*2]; n++;}
    s.data_type=(byte)(bin_freq > (ascii_freq >>> 2) ? Z_BINARY : Z_ASCII);
  }

  static void flush_block_only(Deflate s, boolean eof){
    s._tr_flush_block(s.block_start>=0 ? s.block_start : -1,
                    s.strstart-s.block_start,
                    eof);
    s.block_start=s.strstart;
    s.strm.flush_pending();
  }
}
