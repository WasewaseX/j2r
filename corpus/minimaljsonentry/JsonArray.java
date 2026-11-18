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
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;


/**
 * Represents a JSON array, an ordered collection of JSON values.
 * <p>
 * Elements can be added using the <code>add(...)</code> methods which accept instances of
 * {@link JsonValue}, strings, primitive numbers, and boolean values. To replace an element of an
 * array, use the <code>set(int, ...)</code> methods.
 * </p>
 * <p>
 * Elements can be accessed by their index using {@link #get(int)}. This class also supports
 * iterating over the elements in document order using an {@link #iterator()} or an enhanced for
 * loop:
 * </p>
 * <pre>
 * for (JsonValue value : jsonArray) {
 *   ...
 * }
 * </pre>
 * <p>
 * An equivalent {@link List} can be obtained from the method {@link #values()}.
 * </p>
 * <p>
 * Note that this class is <strong>not thread-safe</strong>. If multiple threads access a
 * <code>JsonArray</code> instance concurrently, while at least one of these threads modifies the
 * contents of this array, access to the instance must be synchronized externally. Failure to do so
 * may lead to an inconsistent state.
 * </p>
 * <p>
 * This class is <strong>not supposed to be extended</strong> by clients.
 * </p>
 *
 * Reshaped from the vendored ralfstx/minimal-json source (scanaislop round-4
 * wave, slice-k): the deprecated static readers, the set(int, ...) family,
 * remove, values() and the iterator moved to JsonArrayAccess as
 * receiver-static helpers so every file stays under the 441-line limit.
 * The upstream javadoc travels with the archived vendor bytes
 * (scripts/evidence/vendor_minimaljson.tar.gz). Fields are package-visible
 * where the family touches them. Arithmetic, control flow and evaluation
 * order are unchanged (byte-identity gated by MinimalJsonCorpus).
 */
@SuppressWarnings("serial") // use default serial UID
public class JsonArray extends JsonValue implements Iterable<JsonValue> {

  final List<JsonValue> values;

  /**
   * Creates a new empty JsonArray.
   */
  public JsonArray() {
    values = new ArrayList<JsonValue>();
  }

  /**
   * Creates a new JsonArray with the contents of the specified JSON array.
   *
   * @param array
   *          the JsonArray to get the initial contents from, must not be <code>null</code>
   */
  public JsonArray(JsonArray array) {
    this(array, false);
  }

  JsonArray(JsonArray array, boolean unmodifiable) {
    if (array == null) {
      throw new NullPointerException("array is null");
    }
    if (unmodifiable) {
      values = Collections.unmodifiableList(array.values);
    } else {
      values = new ArrayList<JsonValue>(array.values);
    }
  }

  /**
   * Appends the JSON representation of the specified <code>int</code> value to the end of this
   * array.
   *
   * @param value
   *          the value to add to the array
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(int value) {
    values.add(Json.value(value));
    return this;
  }

  /**
   * Appends the JSON representation of the specified <code>long</code> value to the end of this
   * array.
   *
   * @param value
   *          the value to add to the array
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(long value) {
    values.add(Json.value(value));
    return this;
  }

  /**
   * Appends the JSON representation of the specified <code>float</code> value to the end of this
   * array.
   *
   * @param value
   *          the value to add to the array
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(float value) {
    values.add(Json.value(value));
    return this;
  }

  /**
   * Appends the JSON representation of the specified <code>double</code> value to the end of this
   * array.
   *
   * @param value
   *          the value to add to the array
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(double value) {
    values.add(Json.value(value));
    return this;
  }

  /**
   * Appends the JSON representation of the specified <code>boolean</code> value to the end of this
   * array.
   *
   * @param value
   *          the value to add to the array
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(boolean value) {
    values.add(Json.value(value));
    return this;
  }

  /**
   * Appends the JSON representation of the specified <code>String</code> value to the end of this
   * array.
   *
   * @param value
   *          the value to add to the array
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(String value) {
    values.add(Json.value(value));
    return this;
  }

  /**
   * Appends the specified JsonValue to the end of this array.
   *
   * @param value
   *          the JsonValue to add to the array, must not be <code>null</code>
   * @return the array itself, to enable method chaining
   */
  public JsonArray add(JsonValue value) {
    if (value == null) {
      throw new NullPointerException("value is null");
    }
    values.add(value);
    return this;
  }

  /**
   * Returns the number of elements in this array.
   *
   * @return the number of elements in this array
   */
  public int size() {
    return values.size();
  }

  /**
   * Returns <code>true</code> if this array contains no elements.
   *
   * @return <code>true</code> if this array contains no elements
   */
  public boolean isEmpty() {
    return values.isEmpty();
  }

  /**
   * Returns the value of the element at the specified position in this array.
   *
   * @param index
   *          the index of the array element to return
   * @return the value of the element at the specified position
   * @throws IndexOutOfBoundsException
   *           if the index is out of range, i.e. <code>index &lt; 0</code> or
   *           <code>index &gt;= size</code>
   */
  public JsonValue get(int index) {
    return values.get(index);
  }

  @Override
  void write(JsonWriter writer) throws IOException {
    writer.writeArrayOpen();
    Iterator<JsonValue> iterator = iterator();
    if (iterator.hasNext()) {
      iterator.next().write(writer);
      while (iterator.hasNext()) {
        writer.writeArraySeparator();
        iterator.next().write(writer);
      }
    }
    writer.writeArrayClose();
  }

  @Override
  public boolean isArray() {
    return true;
  }

  @Override
  public JsonArray asArray() {
    return this;
  }

  @Override
  public int hashCode() {
    return values.hashCode();
  }


  /**
   * Indicates whether a given object is "equal to" this JsonArray. An object is considered equal
   * if it is also a <code>JsonArray</code> and both arrays contain the same list of values.
   * <p>
   * If two JsonArrays are equal, they will also produce the same JSON output.
   * </p>
   *
   * @param object
   *          the object to be compared with this JsonArray
   * @return <tt>true</tt> if the specified object is equal to this JsonArray, <code>false</code>
   *         otherwise
   */
  @Override
  public boolean equals(Object object) {
    if (this == object) {
      return true;
    }
    if (object == null) {
      return false;
    }
    if (getClass() != object.getClass()) {
      return false;
    }
    JsonArray other = (JsonArray)object;
    return values.equals(other.values);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape; the
   * upstream javadoc lives with the implementation).
   */
  public Iterator<JsonValue> iterator() {
    return JsonArrayAccess.iterator(this);
  }

}
