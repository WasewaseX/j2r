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
 * The JZlib 1.1.0 Deflate core (fields, ctor, lm_init, the delegator
 * surface for the DeflateC/Trees/Blocks/Bit/Match/Fast/Slow/Init
 * family, and the deflate() entry refactored into main + header +
 * state-run + trailer helpers - round-3 reshaping, behavior
 * byte-identical: javac+java stdout proof + corpus_suite gate).
 */
public class Deflate implements DeflateC {
  ZStream strm;        // pointer back to this zlib stream
  int status;           // as the name implies
  byte[] pending_buf;   // output still pending
  int pending_buf_size; // size of pending_buf
  int pending_out;      // next pending byte to output to the stream
  int pending;          // nb of bytes in the pending buffer
  int wrap = 1;
  byte data_type;       // UNKNOWN, BINARY or ASCII
  byte method;          // STORED (for zip only) or DEFLATED
  int last_flush;       // value of flush param for previous deflate call

  int w_size;           // LZ77 window size (32K by default)
  int w_bits;           // log2(w_size)  (8..16)
  int w_mask;           // w_size - 1

  byte[] window;
  // Sliding window. Input bytes are read into the second half of the window,
  // and move to the first half later to keep a dictionary of at least wSize
  // bytes. With this organization, matches are limited to a distance of
  // wSize-MAX_MATCH bytes, but this ensures that IO is always
  // performed with a length multiple of the block size. Also, it limits
  // the window size to 64K, which is quite useful on MSDOS.
  // To do: use the user input buffer as sliding window.

  int window_size;
  // Actual size of window: 2*wSize, except when the user input buffer
  // is directly used as sliding window.

  short[] prev;
  // Link to older string with same hash index. To limit the size of this
  // array to 64K, this link is maintained only for the last 32K strings.
  // An index in this array is thus a window index modulo 32K.

  short[] head; // Heads of the hash chains or NIL.

  int ins_h;          // hash index of string to be inserted
  int hash_size;      // number of elements in hash table
  int hash_bits;      // log2(hash_size)
  int hash_mask;      // hash_size-1

  // Number of bits by which ins_h must be shifted at each input
  // step. It must be such that after MIN_MATCH steps, the oldest
  // byte no longer takes part in the hash key, that is:
  // hash_shift * MIN_MATCH >= hash_bits
  int hash_shift;

  // Window position at the beginning of the current output block. Gets
  // negative when the window is moved backwards.

  int block_start;

  int match_length;           // length of best match
  int prev_match;             // previous match
  int match_available;        // set if previous match exists
  int strstart;               // start of string to insert
  int match_start;            // start of matching string
  int lookahead;              // number of valid bytes ahead in window

  // Length of the best match at previous step. Matches not greater than this
  // are discarded. This is used in the lazy match evaluation.
  int prev_length;

  // To speed up deflation, hash chains are never searched beyond this
  // length.  A higher limit improves compression ratio but degrades the speed.
  int max_chain_length;

  // Attempt to find a better match only when the current match is strictly
  // smaller than this value. This mechanism is used only for compression
  // levels >= 4.
  int max_lazy_match;

  // Insert new strings in the hash table only if the match length is not
  // greater than this length. This saves time but degrades compression.
  // max_insert_length is used only for compression levels <= 3.

  int level;    // compression level (1..9)
  int strategy; // favor or force Huffman coding

  // Use a faster search when the previous match is longer than this
  int good_match;

  // Stop searching when current match exceeds this
  int nice_match;

  short[] dyn_ltree;       // literal and length tree
  short[] dyn_dtree;       // distance tree
  short[] bl_tree;         // Huffman tree for bit lengths

  Tree l_desc=new Tree();  // desc for literal tree
  Tree d_desc=new Tree();  // desc for distance tree
  Tree bl_desc=new Tree(); // desc for bit length tree

  // number of codes at each bit length for an optimal tree
  short[] bl_count=new short[MAX_BITS+1];
  // working area to be used in Tree#gen_codes()
  short[] next_code=new short[MAX_BITS+1];

  // heap used to build the Huffman trees
  int[] heap=new int[2*L_CODES+1];

  int heap_len;               // number of elements in the heap
  int heap_max;               // element of largest frequency
  // The sons of heap[n] are heap[2*n] and heap[2*n+1]. heap[0] is not used.
  // The same heap array is used to build all trees.

  // Depth of each subtree used as tie breaker for trees of equal frequency
  byte[] depth=new byte[2*L_CODES+1];

  byte[] l_buf;               // index for literals or lengths */

  // Size of match buffer for literals/lengths.  There are 4 reasons for
  // limiting lit_bufsize to 64K:
  //   - frequencies can be kept in 16 bit counters
  //   - if compression is not successful for the first block, all input
  //     data is still in the window so we can still emit a stored block even
  //     when input comes from standard input.  (This can also be done for
  //     all blocks if lit_bufsize is not greater than 32K.)
  //   - if compression is not successful for a file smaller than 64K, we can
  //     even emit a stored file instead of a stored block (saving 5 bytes).
  //     This is applicable only for zip (not gzip or zlib).
  //   - creating new Huffman trees less frequently may not provide fast
  //     adaptation to changes in the input data statistics. (Take for
  //     example a binary file with poorly compressible code followed by
  //     a highly compressible string table.) Smaller buffer sizes give
  //     fast adaptation but have of course the overhead of transmitting
  //     trees more frequently.
  //   - I can't count above 4
  int lit_bufsize;

  int last_lit;      // running index in l_buf

  // Buffer for distances. To simplify the code, d_buf and l_buf have
  // the same number of elements. To use different lengths, an extra flag
  // array would be necessary.

  int d_buf;         // index of pendig_buf

  int opt_len;        // bit length of current block with optimal trees
  int static_len;     // bit length of current block with static trees
  int matches;        // number of string matches in current block
  int last_eob_len;   // bit length of EOB code for last block

  // Output buffer. bits are inserted starting at the bottom (least
  // significant bits).
  short bi_buf;

  // Number of valid bits in bi_buf.  All bits above the last valid bit
  // are always zero.
  int bi_valid;

  GZIPHeader gheader = null;

  Deflate(ZStream strm){
    this.strm=strm;
    dyn_ltree=new short[HEAP_SIZE*2];
    dyn_dtree=new short[(2*D_CODES+1)*2]; // distance tree
    bl_tree=new short[(2*BL_CODES+1)*2];  // Huffman tree for bit lengths
  }

  void lm_init() {
    window_size=2*w_size;

    head[hash_size-1]=0;
    for(int i=0; i<hash_size-1; i++){
      head[i]=0;
    }

    // Set the default configuration parameters:
    max_lazy_match   = Deflate.config_table[level].max_lazy;
    good_match       = Deflate.config_table[level].good_length;
    nice_match       = Deflate.config_table[level].nice_length;
    max_chain_length = Deflate.config_table[level].max_chain;

    strstart = 0;
    block_start = 0;
    lookahead = 0;
    match_length = prev_length = MIN_MATCH-1;
    match_available = 0;
    ins_h = 0;
  }

  int deflate_stored(int flush){ return DeflateBlocks.deflate_stored(this, flush); }
  int deflate_fast(int flush){ return DeflateFast.deflate_fast(this, flush); }
  int deflate_slow(int flush){ return DeflateSlow.deflate_slow(this, flush); }
  void tr_init(){ DeflateBlocks.tr_init(this); }
  void init_block(){ DeflateBlocks.init_block(this); }
  void _tr_align(){ DeflateBlocks.tr_align(this); }
  void _tr_stored_block(int buf, int stored_len, boolean eof){ DeflateBlocks.tr_stored_block(this, buf, stored_len, eof); }
  void _tr_flush_block(int buf, int stored_len, boolean eof){ DeflateBlocks.tr_flush_block(this, buf, stored_len, eof); }
  boolean _tr_tally(int dist, int lc){ return DeflateBlocks.tr_tally(this, dist, lc); }
  void put_byte(byte[] p, int start, int len){ DeflateBit.put_byte(this, p, start, len); }
  void put_byte(byte c){ DeflateBit.put_byte(this, c); }
  void put_short(int w){ DeflateBit.put_short(this, w); }
  void putShortMSB(int b){ DeflateBit.putShortMSB(this, b); }
  void send_code(int c, short[] tree){ DeflateBit.send_code(this, c, tree); }
  void send_bits(int value, int length){ DeflateBit.send_bits(this, value, length); }
  void bi_flush(){ DeflateBit.bi_flush(this); }
  void bi_windup(){ DeflateBit.bi_windup(this); }
  void set_data_type(){ DeflateBit.set_data_type(this); }
  void flush_block_only(boolean eof){ DeflateBit.flush_block_only(this, eof); }
  int longest_match(int cur_match){ return DeflateMatch.longest_match(this, cur_match); }
  void fill_window(){ DeflateMatch.fill_window(this); }
  int build_bl_tree(){ return DeflateTrees.build_bl_tree(this); }
  void send_all_trees(int lcodes, int dcodes, int blcodes){ DeflateTrees.send_all_trees(this, lcodes, dcodes, blcodes); }
  void scan_tree(short[] tree, int max_code){ DeflateTrees.scan_tree(this, tree, max_code); }
  void send_tree(short[] tree, int max_code){ DeflateTrees.send_tree(this, tree, max_code); }
  void compress_block(short[] ltree, short[] dtree){ DeflateBlocks.compress_block(this, ltree, dtree); }
  void copy_block(int buf, int len, boolean header){ DeflateBlocks.copy_block(this, buf, len, header); }
  void pqdownheap(short[] tree, int v){ DeflateBlocks.pqdownheap(this, tree, v); }
  int deflateInit(int level, int bits, int memlevel){ return DeflateInit.deflateInit(this, level, Z_DEFLATED, bits, memlevel, Z_DEFAULT_STRATEGY); }
  int deflateInit(int level, int bits){ return DeflateInit.deflateInit(this, level, Z_DEFLATED, bits, DEF_MEM_LEVEL, Z_DEFAULT_STRATEGY); }
  int deflateInit(int level){ return DeflateInit.deflateInit(this, level, MAX_WBITS); }
  int deflateReset(){ return DeflateInit.deflateReset(this); }
  int deflateEnd(){ return DeflateInit.deflateEnd(this); }
  int deflateParams(int _level, int _strategy){ return DeflateInit.deflateParams(this, _level, _strategy); }
  int deflateSetDictionary(byte[] dictionary, int dictLength){ return DeflateInit.deflateSetDictionary(this, dictionary, dictLength); }
  static int deflateCopy(ZStream dest, ZStream src){ return DeflateInit.deflateCopy(dest, src); }
  public Object clone() throws CloneNotSupportedException {
    Deflate dest = (Deflate)super.clone();
    DeflateInit.clone_state(this, dest);
    return dest;
  }

  int deflate(int flush){
    int old_flush;

    if(flush>Z_FINISH || flush<0){
      return Z_STREAM_ERROR;
    }

    if(strm.next_out == null ||
       (strm.next_in == null && strm.avail_in != 0) ||
       (status == FINISH_STATE && flush != Z_FINISH)) {
      strm.msg=z_errmsg[Z_NEED_DICT-(Z_STREAM_ERROR)];
      return Z_STREAM_ERROR;
    }
    if(strm.avail_out == 0){
      strm.msg=z_errmsg[Z_NEED_DICT-(Z_BUF_ERROR)];
      return Z_BUF_ERROR;
    }

    old_flush = last_flush;
    last_flush = flush;

    // Write the zlib header
    if(status == INIT_STATE) {
      deflate_write_header();
    }

    // Flush as much pending output as possible
    if(pending != 0) {
      strm.flush_pending();
      if(strm.avail_out == 0) {
        last_flush = -1;
        return Z_OK;
      }
    }
    else if(strm.avail_in==0 && flush <= old_flush &&
            flush != Z_FINISH) {
      strm.msg=z_errmsg[Z_NEED_DICT-(Z_BUF_ERROR)];
      return Z_BUF_ERROR;
    }

    // User must not provide more input after the first FINISH:
    if(status == FINISH_STATE && strm.avail_in != 0) {
      strm.msg=z_errmsg[Z_NEED_DICT-(Z_BUF_ERROR)];
      return Z_BUF_ERROR;
    }

    // Start a new block or continue the current one.
    if(strm.avail_in!=0 || lookahead!=0 ||
       (flush != Z_NO_FLUSH && status != FINISH_STATE)) {
      int r = deflate_run_state(flush);
      if(r != 0){ return r; }
    }

    if(flush!=Z_FINISH) return Z_OK;
    if(wrap<=0) return Z_STREAM_END;

    deflate_write_trailer();

    strm.flush_pending();

    // If avail_out is zero, the application will call deflate again
    // to flush the rest.

    if(wrap > 0) wrap = -wrap; // write the trailer only once!
    return pending != 0 ? Z_OK : Z_STREAM_END;
  }
  // The INIT_STATE header write (zlib or gzip form), extracted verbatim
  // from the upstream deflate() body.
  private void deflate_write_header(){
    if(wrap == 2){
      getGZIPHeader().put(this);
      status=BUSY_STATE;
      strm.adler.reset();
    }
    else{
    int header = (Z_DEFLATED+((w_bits-8)<<4))<<8;
    int level_flags=((level-1)&0xff)>>1;

    if(level_flags>3) level_flags=3;
    header |= (level_flags<<6);
    if(strstart!=0) header |= PRESET_DICT;
    header+=31-(header % 31);

    status=BUSY_STATE;
    putShortMSB(header);

    // Save the adler32 of the preset dictionary:
    if(strstart!=0){
      long adler=strm.adler.getValue();
      putShortMSB((int)(adler>>>16));
      putShortMSB((int)(adler&0xffff));
    }
    strm.adler.reset();
    }
  }
  // The state-machine run (upstream switch over config_table) - returns
  // the Z code to return from deflate(), or 0 to fall through to the tail.
  private int deflate_run_state(int flush){
    int bstate=-1;
    switch(config_table[level].func){
    case STORED:
      bstate = deflate_stored(flush);
      break;
    case FAST:
      bstate = deflate_fast(flush);
      break;
    case SLOW:
      bstate = deflate_slow(flush);
      break;
    default:
    }

    if (bstate==FinishStarted || bstate==FinishDone) {
      status = FINISH_STATE;
    }
    if (bstate==NeedMore || bstate==FinishStarted) {
      if(strm.avail_out == 0) {
        last_flush = -1; // avoid BUF_ERROR next call, see above
      }
      return Z_OK;
    }

    if (bstate==BlockDone) {
      if(flush == Z_PARTIAL_FLUSH) {
        _tr_align();
      }
      else { // FULL_FLUSH or SYNC_FLUSH
        _tr_stored_block(0, 0, false);
        if(flush == Z_FULL_FLUSH) {
          for(int i=0; i<hash_size; i++)  // forget history
            head[i]=0;
        }
      }
      strm.flush_pending();
      if(strm.avail_out == 0) {
        last_flush = -1; // avoid BUF_ERROR at next call, see above
        return Z_OK;
      }
    }
    return 0;
  }
  // The wrap==2 gzip trailer or the zlib adler32 trailer, extracted
  // verbatim from the upstream deflate() body.
  private void deflate_write_trailer(){
    if(wrap==2){
      long adler=strm.adler.getValue();
      put_byte((byte)(adler&0xff));
      put_byte((byte)((adler>>8)&0xff));
      put_byte((byte)((adler>>16)&0xff));
      put_byte((byte)((adler>>24)&0xff));
      put_byte((byte)(strm.total_in&0xff));
      put_byte((byte)((strm.total_in>>8)&0xff));
      put_byte((byte)((strm.total_in>>16)&0xff));
      put_byte((byte)((strm.total_in>>24)&0xff));

      getGZIPHeader().setCRC(adler);
    }
    else{
      // Write the zlib trailer (adler32)
      long adler=strm.adler.getValue();
      putShortMSB((int)(adler>>>16));
      putShortMSB((int)(adler&0xffff));
    }
  }

  synchronized GZIPHeader getGZIPHeader(){
    if(gheader==null){
      gheader = new GZIPHeader();
    }
    return gheader;
  }
}
