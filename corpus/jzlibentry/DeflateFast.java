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
 * DeflateFast - the level 1-3 compression state machine of the
 * JZlib 1.1.0 Deflate (upstream body, refactored under the 80-line
 * function limit: the emit step and the hash-chain insert are
 * helpers; behavior byte-identical - the hash_head value computed
 * inside the emit loop was dead upstream, the insert order and
 * every field update are preserved).
 */
final class DeflateFast implements DeflateC {
  private DeflateFast(){}

  // update ins_h with window[strstart+MIN_MATCH-1] and insert
  // strstart into its hash chain (the upstream 4-line insert block)
  private static void insert_chain(Deflate s){
    s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[(s.strstart)+(MIN_MATCH-1)]&0xff))&s.hash_mask;
    s.prev[s.strstart&s.w_mask]=s.head[s.ins_h];
    s.head[s.ins_h]=(short)s.strstart;
  }

  // the match-emission step: returns bflush (the block-flush flag)
  private static boolean emit(Deflate s){
    boolean bflush;
    if(s.match_length>=MIN_MATCH){
      bflush=s._tr_tally(s.strstart-s.match_start, s.match_length-MIN_MATCH);

      s.lookahead -= s.match_length;

      // Insert new strings in the hash table only if the match length
      // is not too large. This saves time but degrades compression.
      if(s.match_length <= s.max_lazy_match &&
         s.lookahead >= MIN_MATCH) {
        s.match_length--; // string at strstart already in hash table
        do{
          s.strstart++;
          insert_chain(s);
        }
        while (--s.match_length != 0);
        s.strstart++;
      }
      else{
        s.strstart += s.match_length;
        s.match_length = 0;
        s.ins_h = s.window[s.strstart]&0xff;

        s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[s.strstart+1]&0xff))&s.hash_mask;
      }
    }
    else {
      // No match, output a literal byte

      bflush=s._tr_tally(0, s.window[s.strstart]&0xff);
      s.lookahead--;
      s.strstart++;
    }
    return bflush;
  }

  static int deflate_fast(Deflate s, int flush){
    int hash_head = 0; // head of the hash chain
    boolean bflush;      // set if current block must be flushed

    while(true){
      // Make sure that we always have enough lookahead, except
      // at the end of the input file. We need MAX_MATCH bytes
      // for the next match, plus MIN_MATCH bytes to insert the
      // string following the next match.
      if(s.lookahead < MIN_LOOKAHEAD){
        s.fill_window();
        if(s.lookahead < MIN_LOOKAHEAD && flush == Z_NO_FLUSH){
          return NeedMore;
        }
        if(s.lookahead == 0) break; // flush the current block
      }

      // Insert the string window[strstart .. strstart+2] in the
      // dictionary, and set hash_head to the head of the hash chain:
      if(s.lookahead >= MIN_MATCH){
        s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[(s.strstart)+(MIN_MATCH-1)]&0xff))&s.hash_mask;

        hash_head=(s.head[s.ins_h]&0xffff);
        s.prev[s.strstart&s.w_mask]=s.head[s.ins_h];
        s.head[s.ins_h]=(short)s.strstart;
      }

      // Find the longest match, discarding those <= prev_length.
      // At this point we have always match_length < MIN_MATCH

      if(hash_head!=0L &&
         ((s.strstart-hash_head)&0xffff) <= s.w_size-MIN_LOOKAHEAD
         ){
        // To simplify the code, we prevent matches with the string
        // of window index 0 (in particular we have to avoid a match
        // of the string with itself at the start of the input file).
        if(s.strategy != Z_HUFFMAN_ONLY){
          s.match_length=s.longest_match (hash_head);
        }
        // longest_match() sets match_start
      }
      bflush = emit(s);
      if (bflush){

        s.flush_block_only(false);
        if(s.strm.avail_out==0) return NeedMore;
      }
    }

    s.flush_block_only(flush == Z_FINISH);
    if(s.strm.avail_out==0){
      if(flush == Z_FINISH) return FinishStarted;
      else return NeedMore;
    }
    return flush==Z_FINISH ? FinishDone : BlockDone;
  }
}
