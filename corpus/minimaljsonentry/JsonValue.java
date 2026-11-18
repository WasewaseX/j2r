/*******************************************************************************
 * Copyright (c) 2013, 2015 EclipseSource.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 ******************************************************************************/
package minimaljsonentry;

import java.io.IOException;
import java.io.Reader;
import java.io.Serializable;
import java.io.StringWriter;


/**
 * Represents a JSON value. This can be a JSON <strong>object</strong>, an <strong> array</strong>,
 * a <strong>number</strong>, a <strong>string</strong>, or one of the literals
 * <strong>true</strong>, <strong>false</strong>, and <strong>null</strong>.
 * <p>
 * The literals <strong>true</strong>, <strong>false</strong>, and <strong>null</strong> are
 * represented by the constants {@link Json#TRUE}, {@link Json#FALSE}, and {@link Json#NULL}.
 * </p>
 * <p>
 * JSON <strong>objects</strong> and <strong>arrays</strong> are represented by the subtypes
 * {@link JsonObject} and {@link JsonArray}. Instances of these types can be created using the
 * public constructors of these classes.
 * </p>
 * <p>
 * Instances that represent JSON <strong>numbers</strong>, <strong>strings</strong> and
 * <strong>boolean</strong> values can be created using the static factory methods
 * {@link Json#value(String)}, {@link Json#value(long)}, {@link Json#value(double)}, etc.
 * </p>
 * <p>
 * In order to find out whether an instance of this class is of a certain type, the methods
 * {@link #isObject()}, {@link #isArray()}, {@link #isString()}, {@link #isNumber()} etc. can be
 * used.
 * </p>
 * <p>
 * If the type of a JSON value is known, the methods {@link #asObject()}, {@link #asArray()},
 * {@link #asString()}, {@link #asInt()}, etc. can be used to get this value directly in the
 * appropriate target type.
 * </p>
 * <p>
 * This class is <strong>not supposed to be extended</strong> by clients.
 * </p>
 *
 * Reshaped from the vendored ralfstx/minimal-json source (scanaislop round-4
 * wave, slice-k): the as-coercion bodies, the valueOf factories and the
 * writeTo pair moved to JsonValueAccess (receiver-static helpers; the
 * delegators below preserve the polymorphic dispatch); every file stays
 * under the 441-line limit. The upstream javadoc travels with the archived
 * vendor bytes. Arithmetic, control flow and evaluation order are unchanged
 * (byte-identity gated by MinimalJsonCorpus).
 */
@SuppressWarnings("serial") // use default serial UID
public abstract class JsonValue implements Serializable {

  /**
   * Represents the JSON literal <code>true</code>.
   * @deprecated Use <code>Json.TRUE</code> instead
   */
  @Deprecated
  public static final JsonValue TRUE = new JsonLiteral("true");

  /**
   * Represents the JSON literal <code>false</code>.
   * @deprecated Use <code>Json.FALSE</code> instead
   */
  @Deprecated
  public static final JsonValue FALSE = new JsonLiteral("false");

  /**
   * Represents the JSON literal <code>null</code>.
   * @deprecated Use <code>Json.NULL</code> instead
   */
  @Deprecated
  public static final JsonValue NULL = new JsonLiteral("null");

  JsonValue() {
    // prevent subclasses outside of this package
  }

  /**
   * Reads a JSON value from the given reader.
   * <p>
   * Characters are read in chunks and buffered internally, therefore wrapping an existing reader in
   * an additional <code>BufferedReader</code> does <strong>not</strong> improve reading
   * performance.
   * </p>
   *
   * @param reader
   *          the reader to read the JSON value from
   * @return the JSON value that has been read
   * @throws IOException
   *           if an I/O error occurs in the reader
   * @throws ParseException
   *           if the input is not valid JSON
   * @deprecated Use {@link Json#parse(Reader)} instead
   */
  @Deprecated
  public static JsonValue readFrom(Reader reader) throws IOException {
    return Json.parse(reader);
  }

  /**
   * Reads a JSON value from the given text.
   *
   * @param text
   *          the string that contains the JSON value
   * @return the JSON value that has been read
   * @throws ParseException
   *           if the input is not valid JSON
   * @deprecated Use {@link Json#parse(String)} instead
   */
  @Deprecated
  public static JsonValue readFrom(String text) {
    return Json.parse(text);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape; the
   * upstream javadoc lives with the implementation).
   */
  public static JsonValue valueOf(int value) {
    return JsonValueAccess.valueOf(value);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public static JsonValue valueOf(long value) {
    return JsonValueAccess.valueOf(value);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public static JsonValue valueOf(float value) {
    return JsonValueAccess.valueOf(value);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public static JsonValue valueOf(double value) {
    return JsonValueAccess.valueOf(value);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public static JsonValue valueOf(String string) {
    return JsonValueAccess.valueOf(string);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public static JsonValue valueOf(boolean value) {
    return JsonValueAccess.valueOf(value);
  }

  /**
   * Upstream isObject(): base semantics, overridden by the subtypes.
   */
  public boolean isObject() {
    return false;
  }

  /**
   * Upstream isArray().
   */
  public boolean isArray() {
    return false;
  }

  /**
   * Upstream isNumber().
   */
  public boolean isNumber() {
    return false;
  }

  /**
   * Upstream isString().
   */
  public boolean isString() {
    return false;
  }

  /**
   * Upstream isBoolean().
   */
  public boolean isBoolean() {
    return false;
  }

  /**
   * Upstream isTrue().
   */
  public boolean isTrue() {
    return false;
  }

  /**
   * Upstream isFalse().
   */
  public boolean isFalse() {
    return false;
  }

  /**
   * Upstream isNull().
   */
  public boolean isNull() {
    return false;
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape); subtypes
   * keep their overrides and the dynamic dispatch is unchanged.
   */
  public JsonObject asObject() {
    return JsonValueAccess.asObject(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public JsonArray asArray() {
    return JsonValueAccess.asArray(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public int asInt() {
    return JsonValueAccess.asInt(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public long asLong() {
    return JsonValueAccess.asLong(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public float asFloat() {
    return JsonValueAccess.asFloat(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public double asDouble() {
    return JsonValueAccess.asDouble(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public String asString() {
    return JsonValueAccess.asString(this);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape).
   */
  public boolean asBoolean() {
    return JsonValueAccess.asBoolean(this);
  }

  /**
   * Upstream toString(): the JSON string for this value in its minimal
   * form (the result is a valid input for Json.parse and creates an equal
   * value).
   */
  @Override
  public String toString() {
    return toString(WriterConfig.MINIMAL);
  }

  /**
   * Upstream toString(WriterConfig): the JSON string for this value using
   * the given formatting.
   *
   * @param config
   *          a configuration that controls the formatting or <code>null</code> for the minimal form
   * @return a JSON string that represents this value
   */
  public String toString(WriterConfig config) {
    StringWriter writer = new StringWriter();
    try {
      JsonValueAccess.writeTo(this, writer, config);
    } catch (IOException exception) {
      // StringWriter does not throw IOExceptions
      throw new RuntimeException(exception);
    }
    return writer.toString();
  }

  /**
   * Indicates whether some other object is "equal to" this one according to the contract specified
   * in {@link Object#equals(Object)}.
   * <p>
   * Two JsonValues are considered equal if and only if they represent the same JSON text. As a
   * consequence, two given JsonObjects may be different even though they contain the same set of
   * names with the same values, but in a different order.
   * </p>
   *
   * @param object
   *          the reference object with which to compare
   * @return true if this object is the same as the object argument; false otherwise
   */
  @Override
  public boolean equals(Object object) {
    return super.equals(object);
  }

  @Override
  public int hashCode() {
    return super.hashCode();
  }

  abstract void write(JsonWriter writer) throws IOException;

}
