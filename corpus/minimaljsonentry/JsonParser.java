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
import java.io.StringReader;

/**
 * A streaming parser for JSON text. The parser reports all events to a given handler.
 *
 * Reshaped from the vendored ralfstx/minimal-json source (scanaislop round-4
 * wave, slice-k): the string/number token readers moved to JsonParserTokens
 * as receiver-static helpers so every file stays under the 441-line limit.
 * The parser state fields and the state-machine callbacks the helpers call
 * back into are package-visible where the family touches them. Arithmetic,
 * control flow and evaluation order are unchanged (byte-identity gated by
 * MinimalJsonCorpus).
 */
public class JsonParser {

  private static final int MAX_NESTING_LEVEL = 1000;
  private static final int MIN_BUFFER_SIZE = 10;
  private static final int DEFAULT_BUFFER_SIZE = 1024;

  final JsonHandler<Object, Object> handler;
  Reader reader;
  char[] buffer;
  int bufferOffset;
  int index;
  int fill;
  int line;
  int lineOffset;
  int current;
  StringBuilder captureBuffer;
  int captureStart;
  int nestingLevel;

  /*
   * |                      bufferOffset
   *                        v
   * [a|b|c|d|e|f|g|h|i|j|k|l|m|n|o|p|q|r|s|t]        < input
   *                       [l|m|n|o|p|q|r|s|t|?|?]    < buffer
   *                          ^               ^
   *                       |  index           fill
   */

  /**
   * Creates a new JsonParser with the given handler. The parser will report all parser events to
   * this handler.
   *
   * @param handler
   *          the handler to process parser events
   */
  @SuppressWarnings("unchecked")
  public JsonParser(JsonHandler<?, ?> handler) {
    if (handler == null) {
      throw new NullPointerException("handler is null");
    }
    this.handler = (JsonHandler<Object, Object>)handler;
    handler.parser = this;
  }

  /**
   * Parses the given input string. The input must contain a valid JSON value, optionally padded
   * with whitespace.
   *
   * @param string
   *          the input string, must be valid JSON
   * @throws ParseException
   *           if the input is not valid JSON
   */
  public void parse(String string) {
    if (string == null) {
      throw new NullPointerException("string is null");
    }
    int bufferSize = Math.max(MIN_BUFFER_SIZE, Math.min(DEFAULT_BUFFER_SIZE, string.length()));
    try {
      parse(new StringReader(string), bufferSize);
    } catch (IOException exception) {
      // StringReader does not throw IOException
      throw new RuntimeException(exception);
    }
  }

  /**
   * Reads the entire input from the given reader and parses it as JSON. The input must contain a
   * valid JSON value, optionally padded with whitespace.
   * <p>
   * Characters are read in chunks into a default-sized input buffer. Hence, wrapping a reader in an
   * additional <code>BufferedReader</code> likely won't improve reading performance.
   * </p>
   *
   * @param reader
   *          the reader to read the input from
   * @throws IOException
   *           if an I/O error occurs in the reader
   * @throws ParseException
   *           if the input is not valid JSON
   */
  public void parse(Reader reader) throws IOException {
    parse(reader, DEFAULT_BUFFER_SIZE);
  }

  /**
   * Reads the entire input from the given reader and parses it as JSON. The input must contain a
   * valid JSON value, optionally padded with whitespace.
   * <p>
   * Characters are read in chunks into an input buffer of the given size. Hence, wrapping a reader
   * in an additional <code>BufferedReader</code> likely won't improve reading performance.
   * </p>
   *
   * @param reader
   *          the reader to read the input from
   * @param buffersize
   *          the size of the input buffer in chars
   * @throws IOException
   *           if an I/O error occurs in the reader
   * @throws ParseException
   *           if the input is not valid JSON
   */
  public void parse(Reader reader, int buffersize) throws IOException {
    if (reader == null) {
      throw new NullPointerException("reader is null");
    }
    if (buffersize <= 0) {
      throw new IllegalArgumentException("buffersize is zero or negative");
    }
    this.reader = reader;
    buffer = new char[buffersize];
    bufferOffset = 0;
    index = 0;
    fill = 0;
    line = 1;
    lineOffset = 0;
    current = 0;
    captureStart = -1;
    read();
    skipWhiteSpace();
    readValue();
    skipWhiteSpace();
    if (!isEndOfText()) {
      throw error("Unexpected character");
    }
  }

  private void readValue() throws IOException {
    switch (current) {
      case 'n':
        readNull();
        break;
      case 't':
        readTrue();
        break;
      case 'f':
        readFalse();
        break;
      case '"':
        readString();
        break;
      case '[':
        readArray();
        break;
      case '{':
        readObject();
        break;
      case '-':
      case '0':
      case '1':
      case '2':
      case '3':
      case '4':
      case '5':
      case '6':
      case '7':
      case '8':
      case '9':
        JsonParserTokens.readNumber(this);
        break;
      default:
        throw expected("value");
    }
  }

  private void readArray() throws IOException {
    Object array = handler.startArray();
    read();
    if (++nestingLevel > MAX_NESTING_LEVEL) {
      throw error("Nesting too deep");
    }
    skipWhiteSpace();
    if (readChar(']')) {
      nestingLevel--;
      handler.endArray(array);
      return;
    }
    do {
      skipWhiteSpace();
      handler.startArrayValue(array);
      readValue();
      handler.endArrayValue(array);
      skipWhiteSpace();
    } while (readChar(','));
    if (!readChar(']')) {
      throw expected("',' or ']'");
    }
    nestingLevel--;
    handler.endArray(array);
  }

  private void readObject() throws IOException {
    Object object = handler.startObject();
    read();
    if (++nestingLevel > MAX_NESTING_LEVEL) {
      throw error("Nesting too deep");
    }
    skipWhiteSpace();
    if (readChar('}')) {
      nestingLevel--;
      handler.endObject(object);
      return;
    }
    do {
      skipWhiteSpace();
      handler.startObjectName(object);
      String name = readName();
      handler.endObjectName(object, name);
      skipWhiteSpace();
      if (!readChar(':')) {
        throw expected("':'");
      }
      skipWhiteSpace();
      handler.startObjectValue(object, name);
      readValue();
      handler.endObjectValue(object, name);
      skipWhiteSpace();
    } while (readChar(','));
    if (!readChar('}')) {
      throw expected("',' or '}'");
    }
    nestingLevel--;
    handler.endObject(object);
  }

  private String readName() throws IOException {
    if (current != '"') {
      throw expected("name");
    }
    return JsonParserTokens.readStringInternal(this);
  }

  private void readNull() throws IOException {
    handler.startNull();
    read();
    readRequiredChar('u');
    readRequiredChar('l');
    readRequiredChar('l');
    handler.endNull();
  }

  private void readTrue() throws IOException {
    handler.startBoolean();
    read();
    readRequiredChar('r');
    readRequiredChar('u');
    readRequiredChar('e');
    handler.endBoolean(true);
  }

  private void readFalse() throws IOException {
    handler.startBoolean();
    read();
    readRequiredChar('a');
    readRequiredChar('l');
    readRequiredChar('s');
    readRequiredChar('e');
    handler.endBoolean(false);
  }

  private void readRequiredChar(char ch) throws IOException {
    if (!readChar(ch)) {
      throw expected("'" + ch + "'");
    }
  }

  private void readString() throws IOException {
    handler.startString();
    handler.endString(JsonParserTokens.readStringInternal(this));
  }

  boolean readChar(char ch) throws IOException {
    if (current != ch) {
      return false;
    }
    read();
    return true;
  }

  boolean readDigit() throws IOException {
    if (!isDigit()) {
      return false;
    }
    read();
    return true;
  }

  private void skipWhiteSpace() throws IOException {
    while (isWhiteSpace()) {
      read();
    }
  }

  void read() throws IOException {
    if (index == fill) {
      if (captureStart != -1) {
        captureBuffer.append(buffer, captureStart, fill - captureStart);
        captureStart = 0;
      }
      bufferOffset += fill;
      fill = reader.read(buffer, 0, buffer.length);
      index = 0;
      if (fill == -1) {
        current = -1;
        index++;
        return;
      }
    }
    if (current == '\n') {
      line++;
      lineOffset = bufferOffset + index;
    }
    current = buffer[index++];
  }

  void startCapture() {
    if (captureBuffer == null) {
      captureBuffer = new StringBuilder();
    }
    captureStart = index - 1;
  }

  void pauseCapture() {
    int end = current == -1 ? index : index - 1;
    captureBuffer.append(buffer, captureStart, end - captureStart);
    captureStart = -1;
  }

  String endCapture() {
    int start = captureStart;
    int end = index - 1;
    captureStart = -1;
    if (captureBuffer.length() > 0) {
      captureBuffer.append(buffer, start, end - start);
      String captured = captureBuffer.toString();
      captureBuffer.setLength(0);
      return captured;
    }
    return new String(buffer, start, end - start);
  }

  Location getLocation() {
    int offset = bufferOffset + index - 1;
    int column = offset - lineOffset + 1;
    return new Location(offset, line, column);
  }

  ParseException expected(String expected) {
    if (isEndOfText()) {
      return error("Unexpected end of input");
    }
    return error("Expected " + expected);
  }

  private ParseException error(String message) {
    return new ParseException(message, getLocation());
  }

  private boolean isWhiteSpace() {
    return current == ' ' || current == '\t' || current == '\n' || current == '\r';
  }

  boolean isDigit() {
    return current >= '0' && current <= '9';
  }

  private boolean isEndOfText() {
    return current == -1;
  }

}
