package minimaljsonentry;

import java.io.IOException;
import java.io.Reader;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;

/**
 * The access family of {@link JsonArray}: the deprecated static readers,
 * the set(int, ...) family, remove, values() and the iterator, moved
 * verbatim from the vendored ralfstx/minimal-json JsonArray.java as
 * receiver-static helpers (scanaislop round-4 reshape, slice-k) so every
 * file stays under the 441-line limit. The upstream javadoc travels with
 * the archived vendor bytes
 * (scripts/evidence/vendor_minimaljson.tar.gz); the bodies here are
 * byte-faithful with the mx receiver prefixed where the upstream touched
 * instance state. Arithmetic, control flow and evaluation order are
 * unchanged (byte-identity gated by MinimalJsonCorpus).
 */
final class JsonArrayAccess {

  private JsonArrayAccess() {
    // not meant to be instantiated
  }

  /**
   * Upstream readFrom(Reader): reads a JSON array from the given reader
   * (deprecated, delegates through JsonValue.readFrom(...).asArray()).
   */
  @Deprecated
  static JsonArray readFrom(Reader reader) throws IOException {
    return JsonValue.readFrom(reader).asArray();
  }

  /**
   * Upstream readFrom(String): reads a JSON array from the given string
   * (deprecated, delegates through JsonValue.readFrom(...).asArray()).
   */
  @Deprecated
  static JsonArray readFrom(String string) {
    return JsonValue.readFrom(string).asArray();
  }

  /**
   * Upstream unmodifiableArray(JsonArray): an unmodifiable view of the
   * given array.
   */
  static JsonArray unmodifiableArray(JsonArray array) {
    return new JsonArray(array, true);
  }

  /**
   * Upstream set(int, int): replaces the element at the specified
   * position with the JSON representation of the int value.
   */
  static JsonArray set(JsonArray mx, int index, int value) {
    mx.values.set(index, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(int, long).
   */
  static JsonArray set(JsonArray mx, int index, long value) {
    mx.values.set(index, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(int, float).
   */
  static JsonArray set(JsonArray mx, int index, float value) {
    mx.values.set(index, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(int, double).
   */
  static JsonArray set(JsonArray mx, int index, double value) {
    mx.values.set(index, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(int, boolean).
   */
  static JsonArray set(JsonArray mx, int index, boolean value) {
    mx.values.set(index, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(int, String).
   */
  static JsonArray set(JsonArray mx, int index, String value) {
    mx.values.set(index, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(int, JsonValue): replaces the element at the specified
   * position with the given JSON value.
   */
  static JsonArray set(JsonArray mx, int index, JsonValue value) {
    if (value == null) {
      throw new NullPointerException("value is null");
    }
    mx.values.set(index, value);
    return mx;
  }

  /**
   * Upstream remove(int): removes the element at the specified index.
   */
  static JsonArray remove(JsonArray mx, int index) {
    mx.values.remove(index);
    return mx;
  }

  /**
   * Upstream values(): the backed unmodifiable list of this array's values.
   */
  static List<JsonValue> values(JsonArray mx) {
    return Collections.unmodifiableList(mx.values);
  }

  /**
   * Upstream iterator(): the value iterator in document order.
   */
  static Iterator<JsonValue> iterator(JsonArray mx) {
    final Iterator<JsonValue> iterator = mx.values.iterator();
    return new Iterator<JsonValue>() {

      public boolean hasNext() {
        return iterator.hasNext();
      }

      public JsonValue next() {
        return iterator.next();
      }

      public void remove() {
        throw new UnsupportedOperationException();
      }
    };
  }

}
