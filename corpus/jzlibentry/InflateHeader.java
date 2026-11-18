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
 * InflateHeader - the byte-counting and gzip-header states of the
 * JZlib 1.1.0 inflate state machine, plus the stream-reading helpers
 * (upstream bodies, the Inflate receiver made explicit; round-3
 * reshaping, behavior byte-identical).
 */
final class InflateHeader implements InflateC {
  private InflateHeader(){}

  static int inflate_dict4(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need=((s.z.next_in[s.z.next_in_index++]&0xff)<<24)&0xff000000L;
        s.mode=DICT3;
    return r;
  }

  static int inflate_dict3(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need+=((s.z.next_in[s.z.next_in_index++]&0xff)<<16)&0xff0000L;
        s.mode=DICT2;
    return r;
  }

  static int inflate_dict2(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need+=((s.z.next_in[s.z.next_in_index++]&0xff)<<8)&0xff00L;
        s.mode=DICT1;
    return r;
  }

  static int inflate_dict1(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need += (s.z.next_in[s.z.next_in_index++]&0xffL);
        s.z.adler.reset(s.need);
        s.mode = DICT0;
        throw new Return(Z_NEED_DICT);
  }

  static int inflate_dict0(Inflate s, int r, int f) throws Return{
        s.mode = BAD;
        s.z.msg = "s.need dictionary";
        s.marker = 0;       // can try inflateSync
        throw new Return(Z_STREAM_ERROR);
  }

  static int inflate_check4(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need=((s.z.next_in[s.z.next_in_index++]&0xff)<<24)&0xff000000L;
        s.mode=CHECK3;
    return r;
  }

  static int inflate_check3(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need+=((s.z.next_in[s.z.next_in_index++]&0xff)<<16)&0xff0000L;
        s.mode = CHECK2;
    return r;
  }

  static int inflate_check2(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need+=((s.z.next_in[s.z.next_in_index++]&0xff)<<8)&0xff00L;
        s.mode = CHECK1;
    return r;
  }

  static int inflate_check1(Inflate s, int r, int f) throws Return{

        if(s.z.avail_in==0)throw new Return(r);r=f;

        s.z.avail_in--; s.z.total_in++;
        s.need+=(s.z.next_in[s.z.next_in_index++]&0xffL);

        if(s.flags!=0){  // gzip
          s.need = ((s.need&0xff000000)>>24 | 
                          (s.need&0x00ff0000)>>8 | 
                          (s.need&0x0000ff00)<<8 | 
                          (s.need&0x0000ffff)<<24)&0xffffffffL;
        }

        if(((int)(s.was)) != ((int)(s.need))){
          s.z.msg = "incorrect data check";
          // chack is delayed
          /*
          s.mode = BAD;
          s.marker = 5;       // can't try inflateSync
          return r;
          */
        }
        else if(s.flags!=0 && s.gheader!=null){
          s.gheader.crc = s.need; 
        }

        s.mode = LENGTH;
    return r;
  }

  static int inflate_length(Inflate s, int r, int f) throws Return{
        if (s.wrap!=0 && s.flags!=0) {

          try { r=s.readBytes(4, r, f); }
          catch(Return e){ throw e; }

          if(s.z.msg!=null && s.z.msg.equals("incorrect data check")){
            s.mode = BAD;
            s.marker = 5;       // can't try inflateSync
            return r;
          }

          if (s.need != (s.z.total_out & 0xffffffffL)) {
            s.z.msg = "incorrect length check";
            s.mode = BAD;
            return r;
          }
          s.z.msg = null;
        }
        else {
          if(s.z.msg!=null && s.z.msg.equals("incorrect data check")){
            s.mode = BAD;
            s.marker = 5;       // can't try inflateSync
            return r;
          }
        }

        s.mode = DONE;
    return r;
  }

  static int flags(Inflate s, int r, int f) throws Return{

        try { r=s.readBytes(2, r, f); }
        catch(Return e){ throw e; }

        s.flags = ((int)s.need)&0xffff;

        if ((s.flags & 0xff) != Z_DEFLATED) {
          s.z.msg = "unknown compression s.method";
          s.mode = BAD; 
          return r;
        }
        if ((s.flags & 0xe000)!=0) {
          s.z.msg = "unknown header s.flags set";
          s.mode = BAD; 
          return r;
        }

        if ((s.flags & 0x0200)!=0){
          s.checksum(2, s.need);
        } 

        s.mode = TIME;
    return r;
  }

  static int time(Inflate s, int r, int f) throws Return{
        try { r=s.readBytes(4, r, f); }
        catch(Return e){ throw e; }
        if(s.gheader!=null)
          s.gheader.time = s.need;
        if ((s.flags & 0x0200)!=0){
          s.checksum(4, s.need);
        }
        s.mode = OS;
    return r;
  }

  static int os(Inflate s, int r, int f) throws Return{
        try { r=s.readBytes(2, r, f); }
        catch(Return e){ throw e; }
        if(s.gheader!=null){
          s.gheader.xflags = ((int)s.need)&0xff;
          s.gheader.os = (((int)s.need)>>8)&0xff;
        }
        if ((s.flags & 0x0200)!=0){
          s.checksum(2, s.need);
        }
        s.mode = EXLEN;
    return r;
  }

  static int exlen(Inflate s, int r, int f) throws Return{
        if ((s.flags & 0x0400)!=0) {
          try { r=s.readBytes(2, r, f); }
          catch(Return e){ throw e; }
          if(s.gheader!=null){
            s.gheader.extra = new byte[((int)s.need)&0xffff];
          }
          if ((s.flags & 0x0200)!=0){
            s.checksum(2, s.need);
          }
        }
        else if(s.gheader!=null){
          s.gheader.extra=null;
        }
        s.mode = EXTRA;
    return r;
  }

  static int extra(Inflate s, int r, int f) throws Return{
        if ((s.flags & 0x0400)!=0) {
          try { 
            r=s.readBytes(r, f);
            if(s.gheader!=null){
              byte[] foo = s.tmp_string.toByteArray();
              s.tmp_string=null;
              if(foo.length == s.gheader.extra.length){
                System.arraycopy(foo, 0, s.gheader.extra, 0, foo.length);
              }
              else{
                s.z.msg = "bad extra field length";
                s.mode = BAD; 
                return r;
              }
            }
          }
          catch(Return e){ throw e; }
        }
        else if(s.gheader!=null){
          s.gheader.extra=null;
        }
        s.mode = NAME;
    return r;
  }

  static int name(Inflate s, int r, int f) throws Return{
        if ((s.flags & 0x0800)!=0) {
          try { 
            r=s.readString(r, f);
            if(s.gheader!=null){
              s.gheader.name=s.tmp_string.toByteArray();
            }
            s.tmp_string=null;
          }
          catch(Return e){ throw e; }
        }
        else if(s.gheader!=null){
          s.gheader.name=null;
        }
        s.mode = COMMENT;
    return r;
  }

  static int comment(Inflate s, int r, int f) throws Return{
        if ((s.flags & 0x1000)!=0) {
          try { 
            r=s.readString(r, f);
            if(s.gheader!=null){
              s.gheader.comment=s.tmp_string.toByteArray();
            }
            s.tmp_string=null;
          }
          catch(Return e){ throw e; }
        }
        else if(s.gheader!=null){
          s.gheader.comment=null;
        }
        s.mode = HCRC;
    return r;
  }

  static int hcrc(Inflate s, int r, int f) throws Return{
        if ((s.flags & 0x0200)!=0) {
          try { r=s.readBytes(2, r, f); }
          catch(Return e){ throw e; }
          if(s.gheader!=null){
            s.gheader.hcrc=(int)(s.need&0xffff);
          }
          if(s.need != (s.z.adler.getValue()&0xffffL)){
            s.mode = BAD;
            s.z.msg = "header crc mismatch";
            s.marker = 5;       // can't try inflateSync
            return r;
          }
        }
        s.z.adler = new CRC32();

        s.mode = BLOCKS;
        return r;
  }

  static int read_bytes(Inflate s, int n, int r, int f) throws Return{
    if(s.need_bytes == -1){
      s.need_bytes=n;
      s.need=0;
    }
    while(s.need_bytes>0){
      if(s.z.avail_in==0){ throw new Return(r); }; r=f;
      s.z.avail_in--; s.z.total_in++;
      s.need = s.need | 
        ((s.z.next_in[s.z.next_in_index++]&0xff)<<((n-s.need_bytes)*8));
      s.need_bytes--;
    }
    if(n==2){
      s.need&=0xffffL;
    }
    else if(n==4) {
      s.need&=0xffffffffL;
    }
    s.need_bytes=-1;
    return r;
  }

  static int read_string(Inflate s, int r, int f) throws Return{
    if(s.tmp_string == null){
      s.tmp_string=new java.io.ByteArrayOutputStream();
    }
    int b=0; 
    do {
      if(s.z.avail_in==0){ throw new Return(r); }; r=f;
      s.z.avail_in--; s.z.total_in++;
      b = s.z.next_in[s.z.next_in_index];
      if(b!=0) s.tmp_string.write(s.z.next_in, s.z.next_in_index, 1);
      s.z.adler.update(s.z.next_in, s.z.next_in_index, 1);
      s.z.next_in_index++;
    }while(b!=0);
    return r;
  }

  static int read_bytes2(Inflate s, int r, int f) throws Return{
    if(s.tmp_string == null){
      s.tmp_string=new java.io.ByteArrayOutputStream();
    }
    int b=0; 
    while(s.need>0){
      if(s.z.avail_in==0){ throw new Return(r); }; r=f;
      s.z.avail_in--; s.z.total_in++;
      b = s.z.next_in[s.z.next_in_index];
      s.tmp_string.write(s.z.next_in, s.z.next_in_index, 1);
      s.z.adler.update(s.z.next_in, s.z.next_in_index, 1);
      s.z.next_in_index++;
      s.need--;
    }
    return r;
  }

  static void checksum(Inflate s, int n, long v){
    for(int i=0; i<n; i++){
      s.crcbuf[i]=(byte)(v&0xff);
      v>>=8;
    }
    s.z.adler.update(s.crcbuf, 0, n);
  }
}
