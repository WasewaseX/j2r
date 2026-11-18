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
 * DeflateInit - the lifecycle group of the JZlib 1.1.0 Deflate:
 * init/reset/end/params/dictionary plus the copy/clone state
 * duplication (upstream bodies, the Deflate receiver made
 * explicit; round-3 reshaping, behavior byte-identical). The
 * private deflateInit keeps its upstream locals (wrap/level/
 * method/strategy shadow the fields - the explicit this. field
 * writes preserved verbatim).
 */
final class DeflateInit implements DeflateC {
  private DeflateInit(){}

  static int deflateInit(Deflate s, int level, int bits, int memlevel){
    return deflateInit(s, level, Z_DEFLATED, bits, memlevel, Z_DEFAULT_STRATEGY);
  }

  static int deflateInit(Deflate s, int level, int bits){
    return deflateInit(s, level, Z_DEFLATED, bits, DEF_MEM_LEVEL, Z_DEFAULT_STRATEGY);
  }

  static int deflateInit(Deflate s, int level){
    return deflateInit(s, level, MAX_WBITS);
  }

  static int deflateInit(Deflate s, int level, int method, int windowBits, int memLevel, int strategy){
    int wrap = 1;
    //    byte[] my_version=ZLIB_VERSION;

    //
    //  if (version == null || version[0] != my_version[0]
    //  || stream_size != sizeof(z_stream)) {
    //  return Z_VERSION_ERROR;
    //  }

    s.strm.msg = null;

    if (level == Z_DEFAULT_COMPRESSION) level = 6;

    if (windowBits < 0) { // undocumented feature: suppress zlib header
      wrap = 0;
      windowBits = -windowBits;
    }
    else if(windowBits > 15){
      wrap = 2;
      windowBits -= 16;
      s.strm.adler=new CRC32();
    }

    if (memLevel < 1 || memLevel > MAX_MEM_LEVEL || 
        method != Z_DEFLATED ||
        windowBits < 9 || windowBits > 15 || level < 0 || level > 9 ||
        strategy < 0 || strategy > Z_HUFFMAN_ONLY) {
      return Z_STREAM_ERROR;
    }

    s.strm.dstate = (Deflate)s;

    s.wrap = wrap;
    s.w_bits = windowBits;
    s.w_size = 1 << s.w_bits;
    s.w_mask = s.w_size - 1;

    s.hash_bits = memLevel + 7;
    s.hash_size = 1 << s.hash_bits;
    s.hash_mask = s.hash_size - 1;
    s.hash_shift = ((s.hash_bits+MIN_MATCH-1)/MIN_MATCH);

    s.window = new byte[s.w_size*2];
    s.prev = new short[s.w_size];
    s.head = new short[s.hash_size];

    s.lit_bufsize = 1 << (memLevel + 6); // 16K elements by default

    // We overlay pending_buf and d_buf+l_buf. This works since the average
    // output size for (length,distance) codes is <= 24 bits.
    s.pending_buf = new byte[s.lit_bufsize*3];
    s.pending_buf_size = s.lit_bufsize*3;

    s.d_buf = s.lit_bufsize;
    s.l_buf = new byte[s.lit_bufsize];

    s.level = level;

    s.strategy = strategy;
    s.method = (byte)method;

    return s.deflateReset();
  }

  static int deflateReset(Deflate s){
    s.strm.total_in = s.strm.total_out = 0;
    s.strm.msg = null; //
    s.strm.data_type = Z_UNKNOWN;

    s.pending = 0;
    s.pending_out = 0;

    if(s.wrap < 0){
      s.wrap = -s.wrap;
    }
    s.status = (s.wrap==0) ? BUSY_STATE : INIT_STATE;
    s.strm.adler.reset();

    s.last_flush = Z_NO_FLUSH;

    s.tr_init();
    s.lm_init();
    return Z_OK;
  }

  static int deflateEnd(Deflate s){
    if(s.status!=INIT_STATE && s.status!=BUSY_STATE && s.status!=FINISH_STATE){
      return Z_STREAM_ERROR;
    }
    // Deallocate in reverse order of allocations:
    s.pending_buf=null;
    s.l_buf=null;
    s.head=null;
    s.prev=null;
    s.window=null;
    // free
    // dstate=null;
    return s.status == BUSY_STATE ? Z_DATA_ERROR : Z_OK;
  }

  static int deflateParams(Deflate s, int _level, int _strategy){
    int err=Z_OK;

    if(_level == Z_DEFAULT_COMPRESSION){
      _level = 6;
    }
    if(_level < 0 || _level > 9 || 
       _strategy < 0 || _strategy > Z_HUFFMAN_ONLY) {
      return Z_STREAM_ERROR;
    }

    if(config_table[s.level].func!=config_table[_level].func &&
       s.strm.total_in != 0) {
      // Flush the last buffer:
      err = s.strm.deflate(Z_PARTIAL_FLUSH);
    }

    if(s.level != _level) {
      s.level = _level;
      s.max_lazy_match   = config_table[s.level].max_lazy;
      s.good_match       = config_table[s.level].good_length;
      s.nice_match       = config_table[s.level].nice_length;
      s.max_chain_length = config_table[s.level].max_chain;
    }
    s.strategy = _strategy;
    return err;
  }

  static int deflateSetDictionary(Deflate s, byte[] dictionary, int dictLength){
    int length = dictLength;
    int index=0;

    if(dictionary == null || s.status != INIT_STATE)
      return Z_STREAM_ERROR;

    s.strm.adler.update(dictionary, 0, dictLength);

    if(length < MIN_MATCH) return Z_OK;
    if(length > s.w_size-MIN_LOOKAHEAD){
      length = s.w_size-MIN_LOOKAHEAD;
      index=dictLength-length; // use the tail of the dictionary
    }
    System.arraycopy(dictionary, index, s.window, 0, length);
    s.strstart = length;
    s.block_start = length;

    // Insert all strings in the hash table (except for the last two bytes).
    // s->lookahead stays null, so s->ins_h will be recomputed at the next
    // call of fill_window.

    s.ins_h = s.window[0]&0xff;
    s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[1]&0xff))&s.hash_mask;

    for(int n=0; n<=length-MIN_MATCH; n++){
      s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[(n)+(MIN_MATCH-1)]&0xff))&s.hash_mask;
      s.prev[n&s.w_mask]=s.head[s.ins_h];
      s.head[s.ins_h]=(short)n;
    }
    return Z_OK;
  }

  static int deflateCopy(ZStream dest, ZStream src){

    if(src.dstate == null){
      return Z_STREAM_ERROR;
    }

    if(src.next_in!=null){
      dest.next_in = new byte[src.next_in.length];
      System.arraycopy(src.next_in, 0, dest.next_in, 0, src.next_in.length);
    }
    dest.next_in_index = src.next_in_index;
    dest.avail_in = src.avail_in;
    dest.total_in = src.total_in;

    if(src.next_out!=null){
      dest.next_out = new byte[src.next_out.length];
      System.arraycopy(src.next_out, 0, dest.next_out ,0 , src.next_out.length);
    }

    dest.next_out_index = src.next_out_index;
    dest.avail_out = src.avail_out;
    dest.total_out = src.total_out;

    dest.msg = src.msg;
    dest.data_type = src.data_type;
    dest.adler = src.adler.copy();

    try{
      dest.dstate = (Deflate)src.dstate.clone();
      dest.dstate.strm = dest;
    }
    catch(CloneNotSupportedException ignored){
      // statically unreachable: Deflate implements Cloneable (the class
      // declaration above), so src.dstate.clone() cannot refuse; upstream
      // 1.1.0 keeps the defensive catch
    }
    return Z_OK;
  }

  static void clone_state(Deflate s, Deflate dest) throws CloneNotSupportedException {
    dest.pending_buf = dup(dest.pending_buf);
    dest.d_buf = dest.d_buf;
    dest.l_buf = dup(dest.l_buf);
    dest.window = dup(dest.window);

    dest.prev = dup(dest.prev);
    dest.head = dup(dest.head);
    dest.dyn_ltree = dup(dest.dyn_ltree);
    dest.dyn_dtree = dup(dest.dyn_dtree);
    dest.bl_tree = dup(dest.bl_tree);

    dest.bl_count = dup(dest.bl_count);
    dest.next_code = dup(dest.next_code);
    dest.heap = dup(dest.heap);
    dest.depth = dup(dest.depth);

    dest.l_desc.dyn_tree = dest.dyn_ltree;
    dest.d_desc.dyn_tree = dest.dyn_dtree;
    dest.bl_desc.dyn_tree = dest.bl_tree;

    /*
    dest.l_desc.stat_desc = StaticTree.static_l_desc;
    dest.d_desc.stat_desc = StaticTree.static_d_desc;
    dest.bl_desc.stat_desc = StaticTree.static_bl_desc;
    */

    if(dest.gheader!=null){
      dest.gheader = (GZIPHeader)dest.gheader.clone();
    }

  }

  private static byte[] dup(byte[] buf){
    byte[] foo = new byte[buf.length];
    System.arraycopy(buf, 0, foo, 0, foo.length);
    return foo;
  }
  private static short[] dup(short[] buf){
    short[] foo = new short[buf.length];
    System.arraycopy(buf, 0, foo, 0, foo.length);
    return foo;
  }
  private static int[] dup(int[] buf){
    int[] foo = new int[buf.length];
    System.arraycopy(buf, 0, foo, 0, foo.length);
    return foo;
  }
}
