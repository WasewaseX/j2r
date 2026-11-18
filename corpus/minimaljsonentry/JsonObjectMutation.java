package minimaljsonentry;

/**
 * The mutation family of {@link JsonObject}: the add/set/remove members,
 * moved verbatim from the vendored ralfstx/minimal-json JsonObject.java as
 * receiver-static helpers (scanaislop round-4 reshape, slice-k) so every
 * file stays under the 441-line limit. The upstream javadoc for each moved
 * method travels with the archived vendor bytes
 * (scripts/evidence/vendor_minimaljson.tar.gz); the bodies here are
 * byte-faithful with the mx receiver prefixed where the upstream touched
 * instance state. Arithmetic, control flow and evaluation order are
 * unchanged (byte-identity gated by MinimalJsonCorpus).
 */
final class JsonObjectMutation {

  private JsonObjectMutation() {
    // not meant to be instantiated
  }

  /**
   * Appends a new member to the end of this object, with the specified
   * name and the JSON representation of the specified <code>int</code>
   * value (upstream add(String, int)).
   */
  static JsonObject add(JsonObject mx, String name, int value) {
    return add(mx, name, Json.value(value));
  }

  /**
   * Upstream add(String, long).
   */
  static JsonObject add(JsonObject mx, String name, long value) {
    return add(mx, name, Json.value(value));
  }

  /**
   * Upstream add(String, float).
   */
  static JsonObject add(JsonObject mx, String name, float value) {
    return add(mx, name, Json.value(value));
  }

  /**
   * Upstream add(String, double).
   */
  static JsonObject add(JsonObject mx, String name, double value) {
    return add(mx, name, Json.value(value));
  }

  /**
   * Upstream add(String, boolean).
   */
  static JsonObject add(JsonObject mx, String name, boolean value) {
    return add(mx, name, Json.value(value));
  }

  /**
   * Upstream add(String, String).
   */
  static JsonObject add(JsonObject mx, String name, String value) {
    return add(mx, name, Json.value(value));
  }

  /**
   * Appends a new member to the end of this object, with the specified
   * name and the specified JSON value (upstream add(String, JsonValue)).
   */
  static JsonObject add(JsonObject mx, String name, JsonValue value) {
    if (name == null) {
      throw new NullPointerException("name is null");
    }
    if (value == null) {
      throw new NullPointerException("value is null");
    }
    mx.table.add(name, mx.names.size());
    mx.names.add(name);
    mx.values.add(value);
    return mx;
  }

  /**
   * Sets the value of the member with the specified name to the JSON
   * representation of the specified <code>int</code> value (upstream
   * set(String, int); the sibling overloads long/float/double/boolean/
   * String share the shape).
   */
  static JsonObject set(JsonObject mx, String name, int value) {
    set(mx, name, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(String, long).
   */
  static JsonObject set(JsonObject mx, String name, long value) {
    set(mx, name, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(String, float).
   */
  static JsonObject set(JsonObject mx, String name, float value) {
    set(mx, name, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(String, double).
   */
  static JsonObject set(JsonObject mx, String name, double value) {
    set(mx, name, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(String, boolean).
   */
  static JsonObject set(JsonObject mx, String name, boolean value) {
    set(mx, name, Json.value(value));
    return mx;
  }

  /**
   * Upstream set(String, String).
   */
  static JsonObject set(JsonObject mx, String name, String value) {
    set(mx, name, Json.value(value));
    return mx;
  }

  /**
   * Sets the value of the member with the specified name to the specified
   * JSON value (upstream set(String, JsonValue)): replaces the last
   * member with the given name, or appends one at the end.
   */
  static JsonObject set(JsonObject mx, String name, JsonValue value) {
    if (name == null) {
      throw new NullPointerException("name is null");
    }
    if (value == null) {
      throw new NullPointerException("value is null");
    }
    int index = mx.indexOf(name);
    if (index != -1) {
      mx.values.set(index, value);
    } else {
      mx.table.add(name, mx.names.size());
      mx.names.add(name);
      mx.values.add(value);
    }
    return mx;
  }

  /**
   * Removes a member with the specified name from this object (upstream
   * remove(String)): only the last one is removed; the object is not
   * modified when the name is absent.
   */
  static JsonObject remove(JsonObject mx, String name) {
    if (name == null) {
      throw new NullPointerException("name is null");
    }
    int index = mx.indexOf(name);
    if (index != -1) {
      mx.table.remove(index);
      mx.names.remove(index);
      mx.values.remove(index);
    }
    return mx;
  }

}
