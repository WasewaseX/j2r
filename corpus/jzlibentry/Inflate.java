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
 * The JZlib 1.1.0 inflate state machine core: constants, fields,
 * lifecycle, and the inflate() dispatcher refactored from the 377-line
 * upstream case-switch into try{while{switch{case: state-method}}}
 * dispatch (the upstream switch fallthrough is equivalent to
 * set-mode-then-redispatch because the loop body is the switch) -
 * round-3 reshaping, behavior byte-identical.
 */
final class Inflate implements InflateC {

  static final private int MAX_WBITS=15; // 32K LZ77 window

  // preset dictionary flag in zlib header
  static final private int PRESET_DICT=0x20;

  static final int Z_NO_FLUSH=0;
  static final int Z_PARTIAL_FLUSH=1;
  static final int Z_SYNC_FLUSH=2;
  static final int Z_FULL_FLUSH=3;
  static final int Z_FINISH=4;

  static final private int Z_DEFLATED=8;

  static final private int Z_OK=0;
  static final private int Z_STREAM_END=1;
  static final private int Z_NEED_DICT=2;
  static final private int Z_ERRNO=-1;
  static final private int Z_STREAM_ERROR=-2;
  static final private int Z_DATA_ERROR=-3;
  static final private int Z_MEM_ERROR=-4;
  static final private int Z_BUF_ERROR=-5;
  static final private int Z_VERSION_ERROR=-6;

  static final private int METHOD=0;   // waiting for method byte
  static final private int FLAG=1;     // waiting for flag byte
  static final private int DICT4=2;    // four dictionary check bytes to go
  static final private int DICT3=3;    // three dictionary check bytes to go
  static final private int DICT2=4;    // two dictionary check bytes to go
  static final private int DICT1=5;    // one dictionary check byte to go
  static final private int DICT0=6;    // waiting for inflateSetDictionary
  static final private int BLOCKS=7;   // decompressing blocks
  static final private int CHECK4=8;   // four check bytes to go
  static final private int CHECK3=9;   // three check bytes to go
  static final private int CHECK2=10;  // two check bytes to go
  static final private int CHECK1=11;  // one check byte to go
  static final private int DONE=12;    // finished check, done
  static final private int BAD=13;     // got an error--stay here

  static final private int HEAD=14;
  static final private int LENGTH=15;
  static final private int TIME=16;
  static final private int OS=17;
  static final private int EXLEN=18;
  static final private int EXTRA=19;
  static final private int NAME=20;
  static final private int COMMENT=21;
  static final private int HCRC=22;
  static final private int FLAGS=23;

  static final int INFLATE_ANY=0x40000000;

  int mode;                            // current inflate mode

  // mode dependent information
  int method;        // if FLAGS, method byte

  // if CHECK, check values to compare
  long was = -1;           // computed check value
  long need;               // stream check value

  // if BAD, inflateSync's marker bytes count
  int marker;

  // mode independent information
  int  wrap;          // flag for no wrapper
                      // 0: no wrapper
                      // 1: zlib header
                      // 2: gzip header
                      // 4: auto detection

  int wbits;            // log2(window size)  (8..15, defaults to 15)

  InfBlocks blocks;     // current inflate_blocks state

  final ZStream z;

  int flags;

  int need_bytes = -1;
  byte[] crcbuf=new byte[4];

  java.io.ByteArrayOutputStream tmp_string = null;

  GZIPHeader gheader = null;

  int inflateReset(){
    if(z == null) return Z_STREAM_ERROR;
    
    z.total_in = z.total_out = 0;
    z.msg = null;
    this.mode = HEAD;
    this.need_bytes = -1;
    this.blocks.reset();
    return Z_OK;
  }

  int inflateEnd(){
    if(blocks != null){
      blocks.free();
    }
    return Z_OK;
  }

  Inflate(ZStream z){
    this.z=z;
  }

  int inflateInit(int w){
    z.msg = null;
    blocks = null;

    // handle undocumented wrap option (no zlib header or check)
    wrap = 0;
    if(w < 0){
      w = - w;
    }
    else if((w&INFLATE_ANY) != 0){
      wrap = 4;
      w &= ~INFLATE_ANY;
      if(w < 48)
        w &= 15;
    }
    else if((w & ~31) != 0) { // for example, DEF_WBITS + 32
      wrap = 4;               // zlib and gzip wrapped data should be accepted.
      w &= 15;
    }
    else {
      wrap = (w >> 4) + 1;
      if(w < 48)
        w &= 15;
    }

    if(w<8 ||w>15){
      inflateEnd();
      return Z_STREAM_ERROR;
    }
    if(blocks != null && wbits != w){
      blocks.free();
      blocks=null;
    }

    // set window size
    wbits=w;

    this.blocks=new InfBlocks(z, 1<<w);

    // reset state
    inflateReset();

    return Z_OK;
  }

  int inflate(int f){
    int r;

    if(z == null || z.next_in == null){
      if(f == Z_FINISH && this.mode==HEAD)
        return Z_OK; 
      return Z_STREAM_ERROR;
    }

    f = f == Z_FINISH ? Z_BUF_ERROR : Z_OK;
    r = Z_BUF_ERROR;
    try {
      while (true){

        switch (this.mode){
        case HEAD: r = inflate_head(r, f); break;
        case DICT4: r = InflateHeader.inflate_dict4(this, r, f); break;
        case DICT3: r = InflateHeader.inflate_dict3(this, r, f); break;
        case DICT2: r = InflateHeader.inflate_dict2(this, r, f); break;
        case DICT1: r = InflateHeader.inflate_dict1(this, r, f); break;
        case DICT0: r = InflateHeader.inflate_dict0(this, r, f); break;
        case BLOCKS: r = inflate_blocks(r, f); break;
        case CHECK4: r = InflateHeader.inflate_check4(this, r, f); break;
        case CHECK3: r = InflateHeader.inflate_check3(this, r, f); break;
        case CHECK2: r = InflateHeader.inflate_check2(this, r, f); break;
        case CHECK1: r = InflateHeader.inflate_check1(this, r, f); break;
        case LENGTH: r = InflateHeader.inflate_length(this, r, f); break;
        case DONE: r = inflate_done(r, f); break;
        case BAD: r = inflate_bad(r, f); break;
        case FLAGS: r = InflateHeader.flags(this, r, f); break;
        case TIME: r = InflateHeader.time(this, r, f); break;
        case OS: r = InflateHeader.os(this, r, f); break;
        case EXLEN: r = InflateHeader.exlen(this, r, f); break;
        case EXTRA: r = InflateHeader.extra(this, r, f); break;
        case NAME: r = InflateHeader.name(this, r, f); break;
        case COMMENT: r = InflateHeader.comment(this, r, f); break;
        case HCRC: r = InflateHeader.hcrc(this, r, f); break;
        default: r = inflate_default(r, f); break;
        }
      }
    }
    catch(Return e){ return e.r; }
  }

  // HEAD: wrapper detection (upstream case HEAD body, first half)
  private int inflate_head(int r, int f) throws Return{
        if(wrap==0){
          this.mode = BLOCKS;
          return r;
        } 

        try { r=readBytes(2, r, f); }
        catch(Return e){ throw e; }

        if((wrap == 4 || (wrap&2)!=0) &&
           this.need == 0x8b1fL) {   // gzip header
          if(wrap == 4){
            wrap = 2;
          }
          z.adler=new CRC32();
          checksum(2, this.need);

          if(gheader==null) 
            gheader=new GZIPHeader();

          this.mode = FLAGS;
          return r;
        }

        if((wrap&2) != 0){
          this.mode = BAD;
          z.msg = "incorrect header check";
          return r;
        }
    return inflate_head_zlib(r, f);
  }

  // HEAD: the zlib/gzip header validation (upstream case HEAD body,
  // second half; the upstream shared local `b` moves here - it is only
  // read and written inside this state)
  private int inflate_head_zlib(int r, int f) throws Return{
    int b;
        flags = 0;

        this.method = ((int)this.need)&0xff;
        b=((int)(this.need>>8))&0xff;

        if(((wrap&1)==0 ||  // check if zlib header allowed
            (((this.method << 8)+b) % 31)!=0) &&
           (this.method&0xf)!=Z_DEFLATED){
          if(wrap == 4){
            z.next_in_index -= 2;
            z.avail_in += 2;
            z.total_in -= 2;
            wrap = 0;
            this.mode = BLOCKS;
            return r;
          }  
          this.mode = BAD;
          z.msg = "incorrect header check";
          // since zlib 1.2, it is allowted to inflateSync for this case.
          /*
          this.marker = 5;       // can't try inflateSync
          */
          return r;
        }

        if((this.method&0xf)!=Z_DEFLATED){
          this.mode = BAD;
          z.msg="unknown compression method";
          // since zlib 1.2, it is allowted to inflateSync for this case.
          /*
          this.marker = 5;       // can't try inflateSync
          */
          return r;
        }
  
        if(wrap == 4){
          wrap = 1;
        }  

        if((this.method>>4)+8>this.wbits){
          this.mode = BAD;
          z.msg="invalid window size";
          // since zlib 1.2, it is allowted to inflateSync for this case.
          /*
          this.marker = 5;       // can't try inflateSync
          */
          return r;
        }

        z.adler=new Adler32();

        if((b&PRESET_DICT)==0){
          this.mode = BLOCKS;
          return r;
        }
        this.mode = DICT4;
    return r;
  }

  // BLOCKS: the decompression loop state (upstream case BLOCKS body)
  private int inflate_blocks(int r, int f) throws Return{
        r = this.blocks.proc(r);
        if(r == Z_DATA_ERROR){
          this.mode = BAD;
          this.marker = 0;     // can try inflateSync
          return r;
        }
        if(r == Z_OK){
          r = f;
        }
        if(r != Z_STREAM_END){
          throw new Return(r);
        }
        r = f;
        this.was=z.adler.getValue();
        this.blocks.reset();
        if(this.wrap==0){
          this.mode=DONE;
          return r;
        }
        this.mode=CHECK4;
    return r;
  }

  private int inflate_done(int r, int f) throws Return{
    throw new Return(Z_STREAM_END);
  }

  private int inflate_bad(int r, int f) throws Return{
    throw new Return(Z_DATA_ERROR);
  }

  private int inflate_default(int r, int f) throws Return{
    throw new Return(Z_STREAM_ERROR);
  }

  int inflateSetDictionary(byte[] dictionary, int dictLength){
    return InflateSync.inflateSetDictionary(this, dictionary, dictLength);
  }

  int inflateSync(){
    return InflateSync.inflateSync(this);
  }

  int inflateSyncPoint(){
    return InflateSync.inflateSyncPoint(this);
  }

  int readBytes(int n, int r, int f) throws Return{
    return InflateHeader.read_bytes(this, n, r, f);
  }

  int readString(int r, int f) throws Return{
    return InflateHeader.read_string(this, r, f);
  }

  int readBytes(int r, int f) throws Return{
    return InflateHeader.read_bytes2(this, r, f);
  }

  void checksum(int n, long v){
    InflateHeader.checksum(this, n, v);
  }

  public GZIPHeader getGZIPHeader(){
    return gheader;
  }

  boolean inParsingHeader(){
    switch(mode){
      case HEAD:
      case DICT4:
      case DICT3:
      case DICT2:
      case DICT1:
      case FLAGS:
      case TIME:
      case OS:
      case EXLEN:
      case EXTRA:
      case NAME:
      case COMMENT:
      case HCRC:
        return true;
      default:
        return false;
    }
  }
}
