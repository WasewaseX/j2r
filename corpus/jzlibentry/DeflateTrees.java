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
 * DeflateTrees - the bit-length tree scan/send group of the JZlib
 * 1.1.0 Deflate (upstream bodies, the Deflate receiver made
 * explicit; round-3 reshaping, behavior byte-identical).
 */
final class DeflateTrees implements DeflateC {
  private DeflateTrees(){}


  static void scan_tree(Deflate s, short[] tree, int max_code){
    int n;                     // iterates over all tree elements
    int prevlen = -1;          // last emitted length
    int curlen;                // length of current code
    int nextlen = tree[0*2+1]; // length of next code
    int count = 0;             // repeat count of the current code
    int max_count = 7;         // max repeat count
    int min_count = 4;         // min repeat count

    if (nextlen == 0){ max_count = 138; min_count = 3; }
    tree[(max_code+1)*2+1] = (short)0xffff; // guard

    for(n = 0; n <= max_code; n++) {
      curlen = nextlen; nextlen = tree[(n+1)*2+1];
      if(++count < max_count && curlen == nextlen) {
        continue;
      }
      else if(count < min_count) {
        s.bl_tree[curlen*2] += count;
      }
      else if(curlen != 0) {
        if(curlen != prevlen) s.bl_tree[curlen*2]++;
        s.bl_tree[REP_3_6*2]++;
      }
      else if(count <= 10) {
        s.bl_tree[REPZ_3_10*2]++;
      }
      else{
        s.bl_tree[REPZ_11_138*2]++;
      }
      count = 0; prevlen = curlen;
      if(nextlen == 0) {
        max_count = 138; min_count = 3;
      }
      else if(curlen == nextlen) {
        max_count = 6; min_count = 3;
      }
      else{
        max_count = 7; min_count = 4;
      }
    }
  }

  static int build_bl_tree(Deflate s){
    int max_blindex;  // index of last bit length code of non zero freq

    // Determine the bit length frequencies for literal and distance trees
    s.scan_tree(s.dyn_ltree, s.l_desc.max_code);
    s.scan_tree(s.dyn_dtree, s.d_desc.max_code);

    // Build the bit length tree:
    s.bl_desc.build_tree(s);
    // opt_len now includes the length of the tree representations, except
    // the lengths of the bit lengths codes and the 5+5+4 bits for the counts.

    // Determine the number of bit length codes to send. The pkzip format
    // requires that at least 4 bit length codes be sent. (appnote.txt says
    // 3 but the actual value used is 4.)
    for (max_blindex = BL_CODES-1; max_blindex >= 3; max_blindex--) {
      if (s.bl_tree[Tree.bl_order[max_blindex]*2+1] != 0) break;
    }
    // Update opt_len to include the bit length tree and counts
    s.opt_len += 3*(max_blindex+1) + 5+5+4;

    return max_blindex;
  }

  static void send_all_trees(Deflate s, int lcodes, int dcodes, int blcodes){
    int rank;                    // index in bl_order

    s.send_bits(lcodes-257, 5); // not +255 as stated in appnote.txt
    s.send_bits(dcodes-1,   5);
    s.send_bits(blcodes-4,  4); // not -3 as stated in appnote.txt
    for (rank = 0; rank < blcodes; rank++) {
      s.send_bits(s.bl_tree[Tree.bl_order[rank]*2+1], 3);
    }
    s.send_tree(s.dyn_ltree, lcodes-1); // literal tree
    s.send_tree(s.dyn_dtree, dcodes-1); // distance tree
  }

  static void send_tree(Deflate s, short[] tree, int max_code){
    int n;                     // iterates over all tree elements
    int prevlen = -1;          // last emitted length
    int curlen;                // length of current code
    int nextlen = tree[0*2+1]; // length of next code
    int count = 0;             // repeat count of the current code
    int max_count = 7;         // max repeat count
    int min_count = 4;         // min repeat count

    if (nextlen == 0){ max_count = 138; min_count = 3; }

    for (n = 0; n <= max_code; n++) {
      curlen = nextlen; nextlen = tree[(n+1)*2+1];
      if(++count < max_count && curlen == nextlen) {
        continue;
      }
      else if(count < min_count) {
        do { s.send_code(curlen, s.bl_tree); } while (--count != 0);
      }
      else if(curlen != 0){
        if(curlen != prevlen){
          s.send_code(curlen, s.bl_tree); count--;
        }
        s.send_code(REP_3_6, s.bl_tree); 
        s.send_bits(count-3, 2);
      }
      else if(count <= 10){
        s.send_code(REPZ_3_10, s.bl_tree); 
        s.send_bits(count-3, 3);
      }
      else{
        s.send_code(REPZ_11_138, s.bl_tree);
        s.send_bits(count-11, 7);
      }
      count = 0; prevlen = curlen;
      if(nextlen == 0){
        max_count = 138; min_count = 3;
      }
      else if(curlen == nextlen){
        max_count = 6; min_count = 3;
      }
      else{
        max_count = 7; min_count = 4;
      }
    }
  }
}
