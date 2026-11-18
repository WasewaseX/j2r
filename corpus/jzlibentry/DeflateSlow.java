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
 * DeflateSlow - the level 4-9 lazy-evaluation compression state
 * machine of the JZlib 1.1.0 Deflate (upstream body, refactored
 * under the 80-line function limit: the emit branch chain is a
 * helper; behavior byte-identical - the bflush flag never escapes
 * the chain and the hash insert inside the lazy loop was dead
 * local state upstream).
 */
final class DeflateSlow implements DeflateC {
  private DeflateSlow(){}

  // the emit branch chain: returns NeedMore (propagate) or 0 (continue)
  private static int emit_branch(Deflate s){
    boolean bflush;         // set if current block must be flushed

    // If there was a match at the previous step and the current
    // match is not better, output the previous match:
    if(s.prev_length >= MIN_MATCH && s.match_length <= s.prev_length) {
      int max_insert = s.strstart + s.lookahead - MIN_MATCH;
      // Do not insert strings in hash table beyond this.

      bflush=s._tr_tally(s.strstart-1-s.prev_match, s.prev_length - MIN_MATCH);

      // Insert in hash table all strings up to the end of the match.
      // strstart-1 and strstart are already inserted. If there is not
      // enough lookahead, the last two strings are not inserted in
      // the hash table.
      s.lookahead -= s.prev_length-1;
      s.prev_length -= 2;
      do{
        if(++s.strstart <= max_insert) {
          s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[(s.strstart)+(MIN_MATCH-1)]&0xff))&s.hash_mask;
          s.prev[s.strstart&s.w_mask]=s.head[s.ins_h];
          s.head[s.ins_h]=(short)s.strstart;
        }
      }
      while(--s.prev_length != 0);
      s.match_available = 0;
      s.match_length = MIN_MATCH-1;
      s.strstart++;

      if (bflush){
        s.flush_block_only(false);
        if(s.strm.avail_out==0) return NeedMore;
      }
    } else if (s.match_available!=0) {

      // If there was no match at the previous position, output a
      // single literal. If there was a match but the current match
      // is longer, truncate the previous match to a single literal.

      bflush=s._tr_tally(0, s.window[s.strstart-1]&0xff);

      if (bflush) {
        s.flush_block_only(false);
      }
      s.strstart++;
      s.lookahead--;
      if(s.strm.avail_out == 0) return NeedMore;
    } else {
      // There is no previous match to compare with, wait for
      // the next step to decide.

      s.match_available = 1;
      s.strstart++;
      s.lookahead--;
    }
    return 0;
  }

  static int deflate_slow(Deflate s, int flush){
    int hash_head = 0;    // head of hash chain

    // Process the input block.
    while(true){
      // Make sure that we always have enough lookahead, except
      // at the end of the input file. We need MAX_MATCH bytes
      // for the next match, plus MIN_MATCH bytes to insert the
      // string following the next match.

      if (s.lookahead < MIN_LOOKAHEAD) {
        s.fill_window();
        if(s.lookahead < MIN_LOOKAHEAD && flush == Z_NO_FLUSH) {
          return NeedMore;
        }
        if(s.lookahead == 0) break; // flush the current block
      }

      // Insert the string window[strstart .. strstart+2] in the
      // dictionary, and set hash_head to the head of the hash chain:

      if(s.lookahead >= MIN_MATCH) {
        s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[(s.strstart)+(MIN_MATCH-1)]&0xff)) & s.hash_mask;
        hash_head=(s.head[s.ins_h]&0xffff);
        s.prev[s.strstart&s.w_mask]=s.head[s.ins_h];
        s.head[s.ins_h]=(short)s.strstart;
      }

      // Find the longest match, discarding those <= prev_length.
      s.prev_length = s.match_length; s.prev_match = s.match_start;
      s.match_length = MIN_MATCH-1;

      if (hash_head != 0 && s.prev_length < s.max_lazy_match &&
          ((s.strstart-hash_head)&0xffff) <= s.w_size-MIN_LOOKAHEAD
          ){
        // To simplify the code, we prevent matches with the string
        // of window index 0 (in particular we have to avoid a match
        // of the string with itself at the start of the input file).

        if(s.strategy != Z_HUFFMAN_ONLY) {
          s.match_length = s.longest_match(hash_head);
        }
        // longest_match() sets match_start

        if (s.match_length <= 5 && (s.strategy == Z_FILTERED ||
                                  (s.match_length == MIN_MATCH &&
                                   s.strstart - s.match_start > 4096))) {

          // If prev_match is also MIN_MATCH, match_start is garbage
          // but we will ignore the current match anyway.
          s.match_length = MIN_MATCH-1;
        }
      }

      int step = emit_branch(s);
      if (step != 0) return step;
    }

    if(s.match_available!=0) {
      boolean bflush=s._tr_tally(0, s.window[s.strstart-1]&0xff);
      s.match_available = 0;
    }
    s.flush_block_only(flush == Z_FINISH);

    if(s.strm.avail_out==0){
      if(flush == Z_FINISH) return FinishStarted;
      else return NeedMore;
    }

    return flush == Z_FINISH ? FinishDone : BlockDone;
  }
}
