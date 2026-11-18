package minimaljsonentry;

import java.io.IOException;
import java.io.Writer;

/**
 * The access family of {@link JsonValue}: the as-coercion bases, the
 * valueOf factories and the writeTo pair, moved verbatim from the vendored
 * ralfstx/minimal-json JsonValue.java as receiver-static helpers
 * (scanaislop round-4 reshape, slice-k) so every file stays under the
 * 441-line limit. The delegators on JsonValue preserve the polymorphic
 * dispatch (the subtypes' as-override semantics are unchanged). The
 * upstream javadoc travels with the archived vendor bytes. Arithmetic,
 * control flow and evaluation order are unchanged (byte-identity gated by
 * MinimalJsonCorpus).
 */
final class JsonValueAccess {

  private JsonValueAccess() {
    // not meant to be instantiated
  }

  /**
   * Upstream valueOf(int) (deprecated): delegates to Json.value(int).
   */
  @Deprecated
  static JsonValue valueOf(int value) {
    return Json.value(value);
  }

  /**
   * Upstream valueOf(long) (deprecated).
   */
  @Deprecated
  static JsonValue valueOf(long value) {
    return Json.value(value);
  }

  /**
   * Upstream valueOf(float) (deprecated).
   */
  @Deprecated
  static JsonValue valueOf(float value) {
    return Json.value(value);
  }

  /**
   * Upstream valueOf(double) (deprecated).
   */
  @Deprecated
  static JsonValue valueOf(double value) {
    return Json.value(value);
  }

  /**
   * Upstream valueOf(String) (deprecated).
   */
  @Deprecated
  static JsonValue valueOf(String string) {
    return Json.value(string);
  }

  /**
   * Upstream valueOf(boolean) (deprecated).
   */
  @Deprecated
  static JsonValue valueOf(boolean value) {
    return Json.value(value);
  }

  /**
   * Upstream asObject() base: throws for non-object values (the subtype
   * overrides keep their own dispatch through the delegators).
   */
  static JsonObject asObject(JsonValue mx) {
    throw new UnsupportedOperationException("Not an object: " + mx.toString());
  }

  /**
   * Upstream asArray() base.
   */
  static JsonArray asArray(JsonValue mx) {
    throw new UnsupportedOperationException("Not an array: " + mx.toString());
  }

  /**
   * Upstream asInt() base.
   */
  static int asInt(JsonValue mx) {
    throw new UnsupportedOperationException("Not a number: " + mx.toString());
  }

  /**
   * Upstream asLong() base.
   */
  static long asLong(JsonValue mx) {
    throw new UnsupportedOperationException("Not a number: " + mx.toString());
  }

  /**
   * Upstream asFloat() base.
   */
  static float asFloat(JsonValue mx) {
    throw new UnsupportedOperationException("Not a number: " + mx.toString());
  }

  /**
   * Upstream asDouble() base.
   */
  static double asDouble(JsonValue mx) {
    throw new UnsupportedOperationException("Not a number: " + mx.toString());
  }

  /**
   * Upstream asString() base.
   */
  static String asString(JsonValue mx) {
    throw new UnsupportedOperationException("Not a string: " + mx.toString());
  }

  /**
   * Upstream asBoolean() base.
   */
  static boolean asBoolean(JsonValue mx) {
    throw new UnsupportedOperationException("Not a boolean: " + mx.toString());
  }

  /**
   * Upstream writeTo(Writer): writes the JSON representation in minimal
   * form.
   */
  static void writeTo(JsonValue mx, Writer writer) throws IOException {
    writeTo(mx, writer, WriterConfig.MINIMAL);
  }

  /**
   * Upstream writeTo(Writer, WriterConfig): writes the JSON representation
   * using the given formatting through a 128-char WritingBuffer.
   */
  static void writeTo(JsonValue mx, Writer writer, WriterConfig config) throws IOException {
    if (writer == null) {
      throw new NullPointerException("writer is null");
    }
    if (config == null) {
      throw new NullPointerException("config is null");
    }
    WritingBuffer buffer = new WritingBuffer(writer, 128);
    mx.write(config.createWriter(buffer));
    buffer.flush();
  }

}
