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
 * DeflateMatch - the LZ77 window-fill and longest-match group of
 * the JZlib 1.1.0 Deflate (upstream bodies, the Deflate receiver
 * made explicit; round-3 reshaping, behavior byte-identical).
 */
final class DeflateMatch implements DeflateC {
  private DeflateMatch(){}

  static void fill_window(Deflate s){
    int n, m;
    int p;
    int more;    // Amount of free space at the end of the window.

    do{
      more = (s.window_size-s.lookahead-s.strstart);

      // Deal with !@#$% 64K limit:
      if(more==0 && s.strstart==0 && s.lookahead==0){
        more = s.w_size;
      } 
      else if(more==-1) {
        // Very unlikely, but possible on 16 bit machine if strstart == 0
        // and lookahead == 1 (input done one byte at time)
        more--;

        // If the window is almost full and there is insufficient lookahead,
        // move the upper half to the lower one to make room in the upper half.
      }
      else if(s.strstart >= s.w_size+ s.w_size-MIN_LOOKAHEAD) {
        System.arraycopy(s.window, s.w_size, s.window, 0, s.w_size);
        s.match_start-=s.w_size;
        s.strstart-=s.w_size; // we now have strstart >= MAX_DIST
        s.block_start-=s.w_size;

        // Slide the hash table (could be avoided with 32 bit values
        // at the expense of memory usage). We slide even when level == 0
        // to keep the hash table consistent if we switch back to level > 0
        // later. (Using level 0 permanently is not an optimal usage of
        // zlib, so we don't care about this pathological case.)

        n = s.hash_size;
        p=n;
        do {
          m = (s.head[--p]&0xffff);
          s.head[p]=(m>=s.w_size ? (short)(m-s.w_size) : 0);
        }
        while (--n != 0);

        n = s.w_size;
        p = n;
        do {
          m = (s.prev[--p]&0xffff);
          s.prev[p] = (m >= s.w_size ? (short)(m-s.w_size) : 0);
          // If n is not on any hash chain, prev[n] is garbage but
          // its value will never be used.
        }
        while (--n!=0);
        more += s.w_size;
      }

      if (s.strm.avail_in == 0) return;

      // If there was no sliding:
      //    strstart <= WSIZE+MAX_DIST-1 && lookahead <= MIN_LOOKAHEAD - 1 &&
      //    more == window_size - lookahead - strstart
      // => more >= window_size - (MIN_LOOKAHEAD-1 + WSIZE + MAX_DIST-1)
      // => more >= window_size - 2*WSIZE + 2
      // In the BIG_MEM or MMAP case (not yet supported),
      //   window_size == input_size + MIN_LOOKAHEAD  &&
      //   strstart + s->lookahead <= input_size => more >= MIN_LOOKAHEAD.
      // Otherwise, window_size == 2*WSIZE so more >= 2.
      // If there was sliding, more >= WSIZE. So in all cases, more >= 2.

      n = s.strm.read_buf(s.window, s.strstart + s.lookahead, more);
      s.lookahead += n;

      // Initialize the hash value now that we have some input:
      if(s.lookahead >= MIN_MATCH) {
        s.ins_h = s.window[s.strstart]&0xff;
        s.ins_h=(((s.ins_h)<<s.hash_shift)^(s.window[s.strstart+1]&0xff))&s.hash_mask;
      }
      // If the whole input has less than MIN_MATCH bytes, ins_h is garbage,
      // but this is not important since only literal bytes will be emitted.
    }
    while (s.lookahead < MIN_LOOKAHEAD && s.strm.avail_in != 0);
  }

  static int longest_match(Deflate s, int cur_match){
    int chain_length = s.max_chain_length; // max hash chain length
    int scan = s.strstart;                 // current string
    int match;                           // matched string
    int len;                             // length of current match
    int best_len = s.prev_length;          // best match length so far
    int limit = s.strstart>(s.w_size-MIN_LOOKAHEAD) ?
      s.strstart-(s.w_size-MIN_LOOKAHEAD) : 0;
    int nice_match=s.nice_match;

    // Stop when cur_match becomes <= limit. To simplify the code,
    // we prevent matches with the string of window index 0.

    int wmask = s.w_mask;

    int strend = s.strstart + MAX_MATCH;
    byte scan_end1 = s.window[scan+best_len-1];
    byte scan_end = s.window[scan+best_len];

    // The code is optimized for HASH_BITS >= 8 and MAX_MATCH-2 multiple of 16.
    // It is easy to get rid of this optimization if necessary.

    // Do not waste too much time if we already have a good match:
    if (s.prev_length >= s.good_match) {
      chain_length >>= 2;
    }

    // Do not look for matches beyond the end of the input. This is necessary
    // to make deflate deterministic.
    if (nice_match > s.lookahead) nice_match = s.lookahead;

    do {
      match = cur_match;

      // Skip to next match if the match length cannot increase
      // or if the match length is less than 2:
      if (s.window[match+best_len]   != scan_end  ||
          s.window[match+best_len-1] != scan_end1 ||
          s.window[match]       != s.window[scan]     ||
          s.window[++match]     != s.window[scan+1])      continue;

      // The check at best_len-1 can be removed because it will be made
      // again later. (This heuristic is not always a win.)
      // It is not necessary to compare scan[2] and match[2] since they
      // are always equal when the other bytes match, given that
      // the hash keys are equal and that HASH_BITS >= 8.
      scan += 2; match++;

      // We check for insufficient lookahead only every 8th comparison;
      // the 256th check will be made at strstart+258.
      do {
      } while (s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               s.window[++scan] == s.window[++match] &&
               scan < strend);

      len = MAX_MATCH - (int)(strend - scan);
      scan = strend - MAX_MATCH;

      if(len>best_len) {
        s.match_start = cur_match;
        best_len = len;
        if (len >= nice_match) break;
        scan_end1  = s.window[scan+best_len-1];
        scan_end   = s.window[scan+best_len];
      }

    } while ((cur_match = (s.prev[cur_match & wmask]&0xffff)) > limit
             && --chain_length != 0);

    if (best_len <= s.lookahead) return best_len;
    return s.lookahead;
  }
}
