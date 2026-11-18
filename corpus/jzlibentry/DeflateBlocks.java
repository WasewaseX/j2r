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
 * DeflateBlocks - the block-emission group of the JZlib 1.1.0
 * Deflate: tree/dictionary init, stored/static/dynamic block
 * emission, the tally, and the level-0 stored state machine
 * (upstream bodies, the Deflate receiver made explicit;
 * round-3 reshaping, behavior byte-identical).
 */
final class DeflateBlocks implements DeflateC {
  private DeflateBlocks(){}

  static void tr_init(Deflate s){

    s.l_desc.dyn_tree = s.dyn_ltree;
    s.l_desc.stat_desc = StaticTree.static_l_desc;

    s.d_desc.dyn_tree = s.dyn_dtree;
    s.d_desc.stat_desc = StaticTree.static_d_desc;

    s.bl_desc.dyn_tree = s.bl_tree;
    s.bl_desc.stat_desc = StaticTree.static_bl_desc;

    s.bi_buf = 0;
    s.bi_valid = 0;
    s.last_eob_len = 8; // enough lookahead for inflate

    // Initialize the first block of the first file:
    s.init_block();
  }

  static void init_block(Deflate s){
    // Initialize the trees.
    for(int i = 0; i < L_CODES; i++) s.dyn_ltree[i*2] = 0;
    for(int i= 0; i < D_CODES; i++) s.dyn_dtree[i*2] = 0;
    for(int i= 0; i < BL_CODES; i++) s.bl_tree[i*2] = 0;

    s.dyn_ltree[END_BLOCK*2] = 1;
    s.opt_len = s.static_len = 0;
    s.last_lit = s.matches = 0;
  }

  static boolean smaller(short[] tree, int n, int m, byte[] depth){
    short tn2=tree[n*2];
    short tm2=tree[m*2];
    return (tn2<tm2 ||
            (tn2==tm2 && depth[n] <= depth[m]));
  }

  // restore the heap property by moving down the tree starting at
  // node k (upstream pqdownheap; called by Tree.build_tree through
  // the Deflate delegator)
  static void pqdownheap(Deflate s, short[] tree, int k){
    int v = s.heap[k];
    int j = k << 1;  // left son of k
    while (j <= s.heap_len) {
      // Set j to the smallest of the two sons:
      if (j < s.heap_len &&
          smaller(tree, s.heap[j+1], s.heap[j], s.depth)){
        j++;
      }
      // Exit if v is smaller than both sons
      if(smaller(tree, v, s.heap[j], s.depth)) break;

      // Exchange v with the smallest son
      s.heap[k]=s.heap[j];  k = j;
      // And continue down the tree, setting j to the left son of k
      j <<= 1;
    }
    s.heap[k] = v;
  }

  static void tr_align(Deflate s){
    s.send_bits(STATIC_TREES<<1, 3);
    s.send_code(END_BLOCK, StaticTree.static_ltree);

    s.bi_flush();

    // Of the 10 bits for the empty block, we have already sent
    // (10 - bi_valid) bits. The lookahead for the last real code (before
    // the EOB of the previous block) was thus at least one plus the length
    // of the EOB plus what we have just sent of the empty static block.
    if (1 + s.last_eob_len + 10 - s.bi_valid < 9) {
      s.send_bits(STATIC_TREES<<1, 3);
      s.send_code(END_BLOCK, StaticTree.static_ltree);
      s.bi_flush();
    }
    s.last_eob_len = 7;
  }

  static boolean tr_tally(Deflate s, int dist, int lc){

    s.pending_buf[s.d_buf+s.last_lit*2] = (byte)(dist>>>8);
    s.pending_buf[s.d_buf+s.last_lit*2+1] = (byte)dist;

    s.l_buf[s.last_lit] = (byte)lc; s.last_lit++;

    if (dist == 0) {
      // lc is the unmatched char
      s.dyn_ltree[lc*2]++;
    } 
    else {
      s.matches++;
      // Here, lc is the match length - MIN_MATCH
      dist--;             // dist = match distance - 1
      s.dyn_ltree[(Tree._length_code[lc]+LITERALS+1)*2]++;
      s.dyn_dtree[Tree.d_code(dist)*2]++;
    }

    if ((s.last_lit & 0x1fff) == 0 && s.level > 2) {
      // Compute an upper bound for the compressed length
      int out_length = s.last_lit*8;
      int in_length = s.strstart - s.block_start;
      int dcode;
      for (dcode = 0; dcode < D_CODES; dcode++) {
        out_length += (int)s.dyn_dtree[dcode*2] *
          (5L+Tree.extra_dbits[dcode]);
      }
      out_length >>>= 3;
      if ((s.matches < (s.last_lit/2)) && out_length < in_length/2) return true;
    }

    return (s.last_lit == s.lit_bufsize-1);
    // We avoid equality with lit_bufsize because of wraparound at 64K
    // on 16 bit machines and because stored blocks are restricted to
    // 64K-1 bytes.
  }

  static void compress_block(Deflate s, short[] ltree, short[] dtree){
    int  dist;      // distance of matched string
    int lc;         // match length or unmatched char (if dist == 0)
    int lx = 0;     // running index in l_buf
    int code;       // the code to send
    int extra;      // number of extra bits to send

    if (s.last_lit != 0){
      do{
        dist=((s.pending_buf[s.d_buf+lx*2]<<8)&0xff00)|
          (s.pending_buf[s.d_buf+lx*2+1]&0xff);
        lc=(s.l_buf[lx])&0xff; lx++;

        if(dist == 0){
          s.send_code(lc, ltree); // send a literal byte
        } 
        else{
          // Here, lc is the match length - MIN_MATCH
          code = Tree._length_code[lc];

          s.send_code(code+LITERALS+1, ltree); // send the length code
          extra = Tree.extra_lbits[code];
          if(extra != 0){
            lc -= Tree.base_length[code];
            s.send_bits(lc, extra);       // send the extra length bits
          }
          dist--; // dist is now the match distance - 1
          code = Tree.d_code(dist);

          s.send_code(code, dtree);       // send the distance code
          extra = Tree.extra_dbits[code];
          if (extra != 0) {
            dist -= Tree.base_dist[code];
            s.send_bits(dist, extra);   // send the extra distance bits
          }
        } // literal or match pair ?

        // Check that the overlay between pending_buf and d_buf+l_buf is ok:
      }
      while (lx < s.last_lit);
    }

    s.send_code(END_BLOCK, ltree);
    s.last_eob_len = ltree[END_BLOCK*2+1];
  }

  static int deflate_stored(Deflate s, int flush){
    // Stored blocks are limited to 0xffff bytes, pending_buf is limited
    // to pending_buf_size, and each stored block has a 5 byte header:

    int max_block_size = 0xffff;
    int max_start;

    if(max_block_size > s.pending_buf_size - 5) {
      max_block_size = s.pending_buf_size - 5;
    }

    // Copy as much as possible from input to output:
    while(true){
      // Fill the window as much as possible:
      if(s.lookahead<=1){
        s.fill_window();
        if(s.lookahead==0 && flush==Z_NO_FLUSH) return NeedMore;
        if(s.lookahead==0) break; // flush the current block
      }

      s.strstart+=s.lookahead;
      s.lookahead=0;

      // Emit a stored block if pending_buf will be full:
      max_start=s.block_start+max_block_size;
      if(s.strstart==0|| s.strstart>=max_start) {
        // strstart == 0 is possible when wraparound on 16-bit machine
        s.lookahead = (int)(s.strstart-max_start);
        s.strstart = (int)max_start;
      
        s.flush_block_only(false);
        if(s.strm.avail_out==0) return NeedMore;

      }

      // Flush if we may have to slide, otherwise block_start may become
      // negative and the data will be gone:
      if(s.strstart-s.block_start >= s.w_size-MIN_LOOKAHEAD) {
        s.flush_block_only(false);
        if(s.strm.avail_out==0) return NeedMore;
      }
    }

    s.flush_block_only(flush == Z_FINISH);
    if(s.strm.avail_out==0)
      return (flush == Z_FINISH) ? FinishStarted : NeedMore;

    return flush == Z_FINISH ? FinishDone : BlockDone;
  }

  static void tr_stored_block(Deflate s, int buf, int stored_len, boolean eof){
    s.send_bits((STORED_BLOCK<<1)+(eof?1:0), 3);  // send block type
    s.copy_block(buf, stored_len, true);          // with header
  }

  static void tr_flush_block(Deflate s, int buf, int stored_len, boolean eof){
    int opt_lenb, static_lenb;// opt_len and static_len in bytes
    int max_blindex = 0;      // index of last bit length code of non zero freq

    // Build the Huffman trees unless a stored block is forced
    if(s.level > 0) {
      // Check if the file is ascii or binary
      if(s.data_type == Z_UNKNOWN) s.set_data_type();

      // Construct the literal and distance trees
      s.l_desc.build_tree(s);

      s.d_desc.build_tree(s);

      // At this point, opt_len and static_len are the total bit lengths of
      // the compressed block data, excluding the tree representations.

      // Build the bit length tree for the above two trees, and get the index
      // in bl_order of the last bit length code to send.
      max_blindex=s.build_bl_tree();

      // Determine the best encoding. Compute first the block length in bytes
      opt_lenb=(s.opt_len+3+7)>>>3;
      static_lenb=(s.static_len+3+7)>>>3;

      if(static_lenb<=opt_lenb) opt_lenb=static_lenb;
    }
    else {
      opt_lenb=static_lenb=stored_len+5; // force a stored block
    }

    if(stored_len+4<=opt_lenb && buf != -1){
      // 4: two words for the lengths
      // The test buf != NULL is only necessary if LIT_BUFSIZE > WSIZE.
      // Otherwise we can't have processed more than WSIZE input bytes since
      // the last block flush, because compression would have been
      // successful. If LIT_BUFSIZE <= WSIZE, it is never too late to
      // transform a block into a stored block.
      s._tr_stored_block(buf, stored_len, eof);
    }
    else if(static_lenb == opt_lenb){
      s.send_bits((STATIC_TREES<<1)+(eof?1:0), 3);
      s.compress_block(StaticTree.static_ltree, StaticTree.static_dtree);
    }
    else{
      s.send_bits((DYN_TREES<<1)+(eof?1:0), 3);
      s.send_all_trees(s.l_desc.max_code+1, s.d_desc.max_code+1, max_blindex+1);
      s.compress_block(s.dyn_ltree, s.dyn_dtree);
    }

    // The above check is made mod 2^32, for files larger than 512 MB
    // and uLong implemented on 32 bits.

    s.init_block();

    if(eof){
      s.bi_windup();
    }
  }

  static void copy_block(Deflate s, int buf, int len, boolean header){
    int index=0;
    s.bi_windup();      // align on byte boundary
    s.last_eob_len = 8; // enough lookahead for inflate

    if (header) {
      s.put_short((short)len);   
      s.put_short((short)~len);
    }

    //  while(len--!=0) {
    //    put_byte(window[buf+index]);
    //    index++;
    //  }
    s.put_byte(s.window, buf, len);
  }
}
