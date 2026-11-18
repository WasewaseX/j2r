package minimaljsonentry;

import java.util.Collections;
import java.util.Iterator;
import java.util.List;

/**
 * The access family of {@link JsonObject}: contains/merge/get, the typed
 * getters, size/isEmpty/names and the member iterator, moved verbatim from
 * the vendored ralfstx/minimal-json JsonObject.java as receiver-static
 * helpers (scanaislop round-4 reshape, slice-k) so every file stays under
 * the 441-line limit. The upstream javadoc travels with the archived
 * vendor bytes (scripts/evidence/vendor_minimaljson.tar.gz); the bodies
 * here are byte-faithful with the mx receiver prefixed where the upstream
 * touched instance state. HashIndexTable is upstream's package-private
 * static nested class, now a package-private top-level carrier. Arithmetic,
 * control flow and evaluation order are unchanged (byte-identity gated by
 * MinimalJsonCorpus).
 */
final class JsonObjectAccess {

  private JsonObjectAccess() {
    // not meant to be instantiated
  }

  /**
   * Upstream contains(String): checks if a specified member is present as
   * a child of this object.
   */
  static boolean contains(JsonObject mx, String name) {
    return mx.names.contains(name);
  }

  /**
   * Upstream merge(JsonObject): copies all members of the specified object
   * into this object, replacing existing values in place.
   */
  static JsonObject merge(JsonObject mx, JsonObject object) {
    if (object == null) {
      throw new NullPointerException("object is null");
    }
    for (JsonObject.Member member : object) {
      JsonObjectMutation.set(mx, member.name, member.value);
    }
    return mx;
  }

  /**
   * Upstream get(String): returns the value of the member with the
   * specified name, the last one when the name occurs multiple times.
   */
  static JsonValue get(JsonObject mx, String name) {
    if (name == null) {
      throw new NullPointerException("name is null");
    }
    int index = mx.indexOf(name);
    return index != -1 ? mx.values.get(index) : null;
  }

  /**
   * Upstream getInt(String, int).
   */
  static int getInt(JsonObject mx, String name, int defaultValue) {
    JsonValue value = get(mx, name);
    return value != null ? value.asInt() : defaultValue;
  }

  /**
   * Upstream getLong(String, long).
   */
  static long getLong(JsonObject mx, String name, long defaultValue) {
    JsonValue value = get(mx, name);
    return value != null ? value.asLong() : defaultValue;
  }

  /**
   * Upstream getFloat(String, float).
   */
  static float getFloat(JsonObject mx, String name, float defaultValue) {
    JsonValue value = get(mx, name);
    return value != null ? value.asFloat() : defaultValue;
  }

  /**
   * Upstream getDouble(String, double).
   */
  static double getDouble(JsonObject mx, String name, double defaultValue) {
    JsonValue value = get(mx, name);
    return value != null ? value.asDouble() : defaultValue;
  }

  /**
   * Upstream getBoolean(String, boolean).
   */
  static boolean getBoolean(JsonObject mx, String name, boolean defaultValue) {
    JsonValue value = get(mx, name);
    return value != null ? value.asBoolean() : defaultValue;
  }

  /**
   * Upstream getString(String, String).
   */
  static String getString(JsonObject mx, String name, String defaultValue) {
    JsonValue value = get(mx, name);
    return value != null ? value.asString() : defaultValue;
  }

  /**
   * Upstream size(): the number of members (name/value pairs).
   */
  static int size(JsonObject mx) {
    return mx.names.size();
  }

  /**
   * Upstream isEmpty().
   */
  static boolean isEmpty(JsonObject mx) {
    return mx.names.isEmpty();
  }

  /**
   * Upstream names(): the names in document order, backed unmodifiable.
   */
  static List<String> names(JsonObject mx) {
    return Collections.unmodifiableList(mx.names);
  }

  /**
   * Upstream iterator(): the member iterator in document order.
   */
  static Iterator<JsonObject.Member> iterator(JsonObject mx) {
    final Iterator<String> namesIterator = mx.names.iterator();
    final Iterator<JsonValue> valuesIterator = mx.values.iterator();
    return new Iterator<JsonObject.Member>() {

      public boolean hasNext() {
        return namesIterator.hasNext();
      }

      public JsonObject.Member next() {
        String name = namesIterator.next();
        JsonValue value = valuesIterator.next();
        return new JsonObject.Member(name, value);
      }

      public void remove() {
        throw new UnsupportedOperationException();
      }

    };
  }

}
