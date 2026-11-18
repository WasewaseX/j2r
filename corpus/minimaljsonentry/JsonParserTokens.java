package minimaljsonentry;

import java.io.IOException;

/**
 * The string/number token readers of {@link JsonParser}, moved verbatim
 * from the vendored ralfstx/minimal-json JsonParser.java as receiver-static
 * helpers (scanaislop round-4 reshape, slice-k) so every file stays under
 * the 441-line limit. The bodies are byte-faithful with the parser receiver
 * prefixed where the upstream touched instance state; the state-machine
 * callbacks (read/startCapture/pauseCapture/endCapture/expected/readChar/
 * readDigit/isDigit) are package-visible on JsonParser where this family
 * touches them. Arithmetic, control flow and evaluation order are unchanged
 * (byte-identity gated by MinimalJsonCorpus).
 */
final class JsonParserTokens {

  private JsonParserTokens() {
    // not meant to be instantiated
  }

  /**
   * Upstream readStringInternal(): reads a quoted string value with its
   * escape sequences into the capture buffer.
   */
  static String readStringInternal(JsonParser parser) throws IOException {
    parser.read();
    parser.startCapture();
    while (parser.current != '"') {
      if (parser.current == '\\') {
        parser.pauseCapture();
        readEscape(parser);
        parser.startCapture();
      } else if (parser.current < 0x20) {
        throw parser.expected("valid string character");
      } else {
        parser.read();
      }
    }
    String string = parser.endCapture();
    parser.read();
    return string;
  }

  /**
   * Upstream readEscape(): reads an escape sequence after the backslash.
   */
  private static void readEscape(JsonParser parser) throws IOException {
    parser.read();
    switch (parser.current) {
      case '"':
      case '/':
      case '\\':
        parser.captureBuffer.append((char)parser.current);
        break;
      case 'b':
        parser.captureBuffer.append('\b');
        break;
      case 'f':
        parser.captureBuffer.append('\f');
        break;
      case 'n':
        parser.captureBuffer.append('\n');
        break;
      case 'r':
        parser.captureBuffer.append('\r');
        break;
      case 't':
        parser.captureBuffer.append('\t');
        break;
      case 'u':
        char[] hexChars = new char[4];
        for (int i = 0; i < 4; i++) {
          parser.read();
          if (!isHexDigit(parser)) {
            throw parser.expected("hexadecimal digit");
          }
          hexChars[i] = (char)parser.current;
        }
        parser.captureBuffer.append((char)Integer.parseInt(new String(hexChars), 16));
        break;
      default:
        throw parser.expected("valid escape sequence");
    }
    parser.read();
  }

  /**
   * Upstream readNumber(): reads a JSON number literal (integer part,
   * optional fraction and exponent) through the capture buffer.
   */
  static void readNumber(JsonParser parser) throws IOException {
    parser.handler.startNumber();
    parser.startCapture();
    parser.readChar('-');
    int firstDigit = parser.current;
    if (!parser.readDigit()) {
      throw parser.expected("digit");
    }
    if (firstDigit != '0') {
      while (parser.readDigit()) {
      }
    }
    readFraction(parser);
    readExponent(parser);
    parser.handler.endNumber(parser.endCapture());
  }

  /**
   * Upstream readFraction(): the optional fraction part after the dot.
   */
  private static boolean readFraction(JsonParser parser) throws IOException {
    if (!parser.readChar('.')) {
      return false;
    }
    if (!parser.readDigit()) {
      throw parser.expected("digit");
    }
    while (parser.readDigit()) {
    }
    return true;
  }

  /**
   * Upstream readExponent(): the optional exponent part after e/E.
   */
  private static boolean readExponent(JsonParser parser) throws IOException {
    if (!parser.readChar('e') && !parser.readChar('E')) {
      return false;
    }
    if (!parser.readChar('+')) {
      parser.readChar('-');
    }
    if (!parser.readDigit()) {
      throw parser.expected("digit");
    }
    while (parser.readDigit()) {
    }
    return true;
  }

  /**
   * Upstream isHexDigit(): a hexadecimal digit classifier over the current
   * character.
   */
  private static boolean isHexDigit(JsonParser parser) {
    return parser.current >= '0' && parser.current <= '9'
        || parser.current >= 'a' && parser.current <= 'f'
        || parser.current >= 'A' && parser.current <= 'F';
  }

}
