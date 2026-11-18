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
import java.io.ObjectInputStream;
import java.io.Reader;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;

import minimaljsonentry.JsonObject.Member;


/**
 * Represents a JSON object, a set of name/value pairs, where the names are strings and the values
 * are JSON values.
 * <p>
 * Members can be added using the <code>add(String, ...)</code> methods which accept instances of
 * {@link JsonValue}, strings, primitive numbers, and boolean values. To modify certain values of an
 * object, use the <code>set(String, ...)</code> methods. Please note that the <code>add</code>
 * methods are faster than <code>set</code> as they do not search for existing members. On the other
 * hand, the <code>add</code> methods do not prevent adding multiple members with the same name.
 * Duplicate names are discouraged but not prohibited by JSON.
 * </p>
 * <p>
 * Members can be accessed by their name using {@link #get(String)}. A list of all names can be
 * obtained from the method {@link #names()}. This class also supports iterating over the members in
 * document order using an {@link #iterator()} or an enhanced for loop:
 * </p>
 * <pre>
 * for (Member member : jsonObject) {
 *   String name = member.getName();
 *   JsonValue value = member.getValue();
 *   ...
 * }
 * </pre>
 * <p>
 * Even though JSON objects are unordered by definition, instances of this class preserve the order
 * of members to allow processing in document order and to guarantee a predictable output.
 * </p>
 * <p>
 * Note that this class is <strong>not thread-safe</strong>. If multiple threads access a
 * <code>JsonObject</code> instance concurrently, while at least one of these threads modifies the
 * contents of this object, access to the instance must be synchronized externally. Failure to do so
 * may lead to an inconsistent state.
 * </p>
 * <p>
 * This class is <strong>not supposed to be extended</strong> by clients.
 * </p>
 *
 * Reshaped from the vendored ralfstx/minimal-json source (scanaislop round-4
 * wave): the add/set/remove mutation family moved to JsonObjectMutation, the
 * access/iteration family to JsonObjectAccess, and HashIndexTable to a
 * package-private top-level carrier in JsonObjectAccess.java, so every file
 * stays under the 441-line limit. The upstream javadoc lives with the moved
 * implementation; the referenced API keeps one-line delegators here. Fields
 * are package-visible where the family touches them. Arithmetic, control flow
 * and evaluation order are unchanged (byte-identity gated by
 * MinimalJsonCorpus).
 */
@SuppressWarnings("serial") // use default serial UID
public class JsonObject extends JsonValue implements Iterable<Member> {

  final List<String> names;
  final List<JsonValue> values;
  transient HashIndexTable table;

  /**
   * Creates a new empty JsonObject.
   */
  public JsonObject() {
    names = new ArrayList<String>();
    values = new ArrayList<JsonValue>();
    table = new HashIndexTable();
  }

  /**
   * Creates a new JsonObject, initialized with the contents of the specified JSON object.
   *
   * @param object
   *          the JSON object to get the initial contents from, must not be <code>null</code>
   */
  public JsonObject(JsonObject object) {
    this(object, false);
  }

  private JsonObject(JsonObject object, boolean unmodifiable) {
    if (object == null) {
      throw new NullPointerException("object is null");
    }
    if (unmodifiable) {
      names = Collections.unmodifiableList(object.names);
      values = Collections.unmodifiableList(object.values);
    } else {
      names = new ArrayList<String>(object.names);
      values = new ArrayList<JsonValue>(object.values);
    }
    table = new HashIndexTable();
    updateHashIndex();
  }

  /**
   * Reads a JSON object from the given reader.
   * <p>
   * Characters are read in chunks and buffered internally, therefore wrapping an existing reader in
   * an additional <code>BufferedReader</code> does <strong>not</strong> improve reading
   * performance.
   * </p>
   *
   * @param reader
   *          the reader to read the JSON object from
   * @return the JSON object that has been read
   * @throws IOException
   *           if an I/O error occurs in the reader
   * @throws ParseException
   *           if the input is not valid JSON
   * @throws UnsupportedOperationException
   *           if the input does not contain a JSON object
   * @deprecated Use {@link Json#parse(Reader)}{@link JsonValue#asObject() .asObject()} instead
   */
  @Deprecated
  public static JsonObject readFrom(Reader reader) throws IOException {
    return JsonValue.readFrom(reader).asObject();
  }

  /**
   * Reads a JSON object from the given string.
   *
   * @param string
   *          the string that contains the JSON object
   * @return the JSON object that has been read
   * @throws ParseException
   *           if the input is not valid JSON
   * @throws UnsupportedOperationException
   *           if the input does not contain a JSON object
   * @deprecated Use {@link Json#parse(String)}{@link JsonValue#asObject() .asObject()} instead
   */
  @Deprecated
  public static JsonObject readFrom(String string) {
    return JsonValue.readFrom(string).asObject();
  }

  /**
   * Returns an unmodifiable JsonObject for the specified one. This method allows to provide
   * read-only access to a JsonObject.
   * <p>
   * The returned JsonObject is backed by the given object and reflect changes that happen to it.
   * Attempts to modify the returned JsonObject result in an
   * <code>UnsupportedOperationException</code>.
   * </p>
   *
   * @param object
   *          the JsonObject for which an unmodifiable JsonObject is to be returned
   * @return an unmodifiable view of the specified JsonObject
   */
  public static JsonObject unmodifiableObject(JsonObject object) {
    return new JsonObject(object, true);
  }

  /**
   * Delegates to the mutation helper (scanaislop round-4 reshape; the
   * upstream javadoc lives with the implementation).
   */
  public JsonObject add(String name, int value) {
    return JsonObjectMutation.add(this, name, value);
  }

  /**
   * Delegates to the mutation helper (scanaislop round-4 reshape).
   */
  public JsonObject add(String name, String value) {
    return JsonObjectMutation.add(this, name, value);
  }

  /**
   * Delegates to the mutation helper (scanaislop round-4 reshape).
   */
  public JsonObject add(String name, JsonValue value) {
    return JsonObjectMutation.add(this, name, value);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape; the
   * upstream javadoc lives with the implementation).
   */
  public JsonValue get(String name) {
    return JsonObjectAccess.get(this, name);
  }

  /**
   * Delegates to the access helper (scanaislop round-4 reshape; the
   * Iterable contract method).
   */
  public Iterator<Member> iterator() {
    return JsonObjectAccess.iterator(this);
  }

  @Override
  void write(JsonWriter writer) throws IOException {
    writer.writeObjectOpen();
    Iterator<String> namesIterator = names.iterator();
    Iterator<JsonValue> valuesIterator = values.iterator();
    if (namesIterator.hasNext()) {
      writer.writeMemberName(namesIterator.next());
      writer.writeMemberSeparator();
      valuesIterator.next().write(writer);
      while (namesIterator.hasNext()) {
        writer.writeObjectSeparator();
        writer.writeMemberName(namesIterator.next());
        writer.writeMemberSeparator();
        valuesIterator.next().write(writer);
      }
    }
    writer.writeObjectClose();
  }

  @Override
  public boolean isObject() {
    return true;
  }

  @Override
  public JsonObject asObject() {
    return this;
  }

  @Override
  public int hashCode() {
    int result = 1;
    result = 31 * result + names.hashCode();
    result = 31 * result + values.hashCode();
    return result;
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (obj == null) {
      return false;
    }
    if (getClass() != obj.getClass()) {
      return false;
    }
    JsonObject other = (JsonObject)obj;
    return names.equals(other.names) && values.equals(other.values);
  }

  int indexOf(String name) {
    int index = table.get(name);
    if (index != -1 && name.equals(names.get(index))) {
      return index;
    }
    return names.lastIndexOf(name);
  }

  private synchronized void readObject(ObjectInputStream inputStream)
      throws IOException, ClassNotFoundException
  {
    inputStream.defaultReadObject();
    table = new HashIndexTable();
    updateHashIndex();
  }

  private void updateHashIndex() {
    int size = names.size();
    for (int i = 0; i < size; i++) {
      table.add(names.get(i), i);
    }
  }

  /**
   * Represents a member of a JSON object, a pair of a name and a value.
   * Fields are package-visible where the reshaped family touches them
   * (scanaislop round-4 reshape).
   */
  public static class Member {

    final String name;
    final JsonValue value;

    Member(String name, JsonValue value) {
      this.name = name;
      this.value = value;
    }

    /**
     * Returns the name of this member.
     *
     * @return the name of this member, never <code>null</code>
     */
    public String getName() {
      return name;
    }

    /**
     * Returns the value of this member.
     *
     * @return the value of this member, never <code>null</code>
     */
    public JsonValue getValue() {
      return value;
    }

    @Override
    public int hashCode() {
      int result = 1;
      result = 31 * result + name.hashCode();
      result = 31 * result + value.hashCode();
      return result;
    }

    /**
     * Indicates whether a given object is "equal to" this JsonObject. An object is considered equal
     * if it is also a <code>JsonObject</code> and both objects contain the same members <em>in
     * the same order</em>.
     * <p>
     * If two JsonObjects are equal, they will also produce the same JSON output.
     * </p>
     *
     * @param object
     *          the object to be compared with this JsonObject
     * @return <tt>true</tt> if the specified object is equal to this JsonObject, <code>false</code>
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
      Member other = (Member)object;
      return name.equals(other.name) && value.equals(other.value);
    }

  }

}
