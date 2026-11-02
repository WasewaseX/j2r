import java.util.ArrayList;
import java.util.HashMap;

/**
 * BigJson — a real-program-scale differential fixture: a JSON parser,
 * serializer and pretty-printer built on the supported JDK surface.
 *
 * Structure: lexer + recursive-descent parser into a JsonValue class
 * hierarchy, then a compact serializer and an indented pretty-printer,
 * then a round-trip check and a corpus of documents (including malformed
 * ones, whose error messages must match the JVM byte-for-byte).
 *
 * No regex, no String.format, no java.util.Random — everything is
 * hand-rolled so the whole transcript is deterministic.
 */
public class BigJson {

    // ------------------------------------------------------------------
    // Values
    // ------------------------------------------------------------------

    abstract static class JsonValue {
        abstract void serialize(StringBuilder sb);
    }

    static final class JsonNull extends JsonValue {
        void serialize(StringBuilder sb) {
            sb.append("null");
        }
    }

    static final class JsonBool extends JsonValue {
        final boolean value;
        JsonBool(boolean value) {
            this.value = value;
        }
        void serialize(StringBuilder sb) {
            sb.append(value ? "true" : "false");
        }
    }

    /** Numbers keep the raw text so re-serialization is lossless. */
    static final class JsonNumber extends JsonValue {
        final String raw;
        JsonNumber(String raw) {
            this.raw = raw;
        }
        void serialize(StringBuilder sb) {
            sb.append(raw);
        }
        /** Numbers compare by their raw text — the round-trip re-serializes
         * the exact input, so text equality is the stricter check. */
        String asText() {
            return raw;
        }
    }

    static final class JsonString extends JsonValue {
        final String value;
        JsonString(String value) {
            this.value = value;
        }
        void serialize(StringBuilder sb) {
            appendQuoted(sb, value);
        }
    }

    static final class JsonArray extends JsonValue {
        final ArrayList<JsonValue> items = new ArrayList<JsonValue>();
        void serialize(StringBuilder sb) {
            sb.append('[');
            for (int i = 0; i < items.size(); i++) {
                if (i > 0) {
                    sb.append(',');
                }
                items.get(i).serialize(sb);
            }
            sb.append(']');
        }
    }

    static final class JsonObject extends JsonValue {
        final HashMap<String, JsonValue> members = new HashMap<String, JsonValue>();
        /** Insertion order, kept for deterministic pretty-printing. */
        final ArrayList<String> keyOrder = new ArrayList<String>();
        void put(String key, JsonValue v) {
            if (!members.containsKey(key)) {
                keyOrder.add(key);
            }
            members.put(key, v);
        }
        JsonValue get(String key) {
            return members.get(key);
        }
        void serialize(StringBuilder sb) {
            sb.append('{');
            boolean first = true;
            for (int i = 0; i < keyOrder.size(); i++) {
                String k = keyOrder.get(i);
                if (!first) {
                    sb.append(',');
                }
                first = false;
                appendQuoted(sb, k);
                sb.append(':');
                members.get(k).serialize(sb);
            }
            sb.append('}');
        }
    }

    // ------------------------------------------------------------------
    // Errors
    // ------------------------------------------------------------------

    static class JsonParseException extends RuntimeException {
        JsonParseException(String message, int pos) {
            super(message + " at position " + pos);
        }
    }

    // ------------------------------------------------------------------
    // Lexer + parser
    // ------------------------------------------------------------------

    static final class Parser {
        final String src;
        int pos;

        Parser(String src) {
            this.src = src;
        }

        void skipWs() {
            while (pos < src.length()) {
                char c = src.charAt(pos);
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                    pos++;
                } else {
                    break;
                }
            }
        }

        char peek() {
            if (pos >= src.length()) {
                throw new JsonParseException("unexpected end of input", pos);
            }
            return src.charAt(pos);
        }

        char next() {
            char c = peek();
            pos++;
            return c;
        }

        void expect(char c) {
            char got = next();
            if (got != c) {
                throw new JsonParseException("expected '" + c + "' but got '" + got + "'", pos - 1);
            }
        }

        JsonValue parseValue() {
            skipWs();
            char c = peek();
            switch (c) {
                case '{':
                    return parseObject();
                case '[':
                    return parseArray();
                case '"':
                    return new JsonString(parseString());
                case 't':
                    expectWord("true");
                    return new JsonBool(true);
                case 'f':
                    expectWord("false");
                    return new JsonBool(false);
                case 'n':
                    expectWord("null");
                    return new JsonNull();
                default:
                    return parseNumber();
            }
        }

        void expectWord(String w) {
            for (int i = 0; i < w.length(); i++) {
                if (pos >= src.length() || src.charAt(pos) != w.charAt(i)) {
                    throw new JsonParseException("invalid literal, wanted " + w, pos);
                }
                pos++;
            }
        }

        JsonObject parseObject() {
            expect('{');
            JsonObject obj = new JsonObject();
            skipWs();
            if (peek() == '}') {
                pos++;
                return obj;
            }
            while (true) {
                skipWs();
                String key = parseString();
                skipWs();
                expect(':');
                JsonValue v = parseValue();
                obj.put(key, v);
                skipWs();
                char c = next();
                if (c == '}') {
                    return obj;
                }
                if (c != ',') {
                    throw new JsonParseException("expected ',' or '}' but got '" + c + "'", pos - 1);
                }
            }
        }

        JsonArray parseArray() {
            expect('[');
            JsonArray arr = new JsonArray();
            skipWs();
            if (peek() == ']') {
                pos++;
                return arr;
            }
            while (true) {
                arr.items.add(parseValue());
                skipWs();
                char c = next();
                if (c == ']') {
                    return arr;
                }
                if (c != ',') {
                    throw new JsonParseException("expected ',' or ']' but got '" + c + "'", pos - 1);
                }
            }
        }

        String parseString() {
            expect('"');
            StringBuilder sb = new StringBuilder();
            while (true) {
                char c = next();
                if (c == '"') {
                    return sb.toString();
                }
                if (c == '\\') {
                    char esc = next();
                    if (esc == '"') {
                        sb.append('"');
                    } else if (esc == '\\') {
                        sb.append('\\');
                    } else if (esc == '/') {
                        sb.append('/');
                    } else if (esc == 'b') {
                        sb.append('\b');
                    } else if (esc == 'f') {
                        sb.append('\f');
                    } else if (esc == 'n') {
                        sb.append('\n');
                    } else if (esc == 'r') {
                        sb.append('\r');
                    } else if (esc == 't') {
                        sb.append('\t');
                    } else if (esc == 'u') {
                        int code = 0;
                        for (int i = 0; i < 4; i++) {
                            char h = next();
                            int d = hexDigit(h);
                            code = code * 16 + d;
                        }
                        sb.append((char) code);
                    } else {
                        throw new JsonParseException("bad escape '\\" + esc + "'", pos - 2);
                    }
                } else {
                    sb.append(c);
                }
            }
        }

        int hexDigit(char c) {
            if (c >= '0' && c <= '9') {
                return c - '0';
            }
            if (c >= 'a' && c <= 'f') {
                return c - 'a' + 10;
            }
            if (c >= 'A' && c <= 'F') {
                return c - 'A' + 10;
            }
            throw new JsonParseException("bad hex digit '" + c + "'", pos);
        }

        JsonNumber parseNumber() {
            int start = pos;
            if (peek() == '-') {
                pos++;
            }
            while (pos < src.length()) {
                char c = src.charAt(pos);
                if ((c >= '0' && c <= '9') || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-') {
                    pos++;
                } else {
                    break;
                }
            }
            if (pos == start) {
                throw new JsonParseException("unexpected character '" + peek() + "'", pos);
            }
            return new JsonNumber(src.substring(start, pos));
        }
    }

    // ------------------------------------------------------------------
    // Serialization helpers
    // ------------------------------------------------------------------

    static void appendQuoted(StringBuilder sb, String s) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"') {
                sb.append("\\\"");
            } else if (c == '\\') {
                sb.append("\\\\");
            } else if (c == '\n') {
                sb.append("\\n");
            } else if (c == '\t') {
                sb.append("\\t");
            } else if (c == '\r') {
                sb.append("\\r");
            } else {
                sb.append(c);
            }
        }
        sb.append('"');
    }

    static void prettyPrint(StringBuilder sb, JsonValue v, int indent) {
        if (v instanceof JsonObject) {
            JsonObject obj = (JsonObject) v;
            if (obj.keyOrder.isEmpty()) {
                sb.append("{}");
                return;
            }
            sb.append("{\n");
            for (int i = 0; i < obj.keyOrder.size(); i++) {
                String k = obj.keyOrder.get(i);
                pad(sb, indent + 2);
                appendQuoted(sb, k);
                sb.append(": ");
                prettyPrint(sb, obj.members.get(k), indent + 2);
                if (i < obj.keyOrder.size() - 1) {
                    sb.append(',');
                }
                sb.append('\n');
            }
            pad(sb, indent);
            sb.append('}');
        } else if (v instanceof JsonArray) {
            JsonArray arr = (JsonArray) v;
            if (arr.items.isEmpty()) {
                sb.append("[]");
                return;
            }
            sb.append("[\n");
            for (int i = 0; i < arr.items.size(); i++) {
                pad(sb, indent + 2);
                prettyPrint(sb, arr.items.get(i), indent + 2);
                if (i < arr.items.size() - 1) {
                    sb.append(',');
                }
                sb.append('\n');
            }
            pad(sb, indent);
            sb.append(']');
        } else {
            v.serialize(sb);
        }
    }

    static void pad(StringBuilder sb, int n) {
        for (int i = 0; i < n; i++) {
            sb.append(' ');
        }
    }

    /** Structural equality (numbers compare by double value). */
    static boolean jsonEquals(JsonValue a, JsonValue b) {
        if (a instanceof JsonObject && b instanceof JsonObject) {
            JsonObject oa = (JsonObject) a;
            JsonObject ob = (JsonObject) b;
            if (oa.keyOrder.size() != ob.keyOrder.size()) {
                return false;
            }
            for (int i = 0; i < oa.keyOrder.size(); i++) {
                String k = oa.keyOrder.get(i);
                if (!ob.members.containsKey(k)) {
                    return false;
                }
                if (!jsonEquals(oa.members.get(k), ob.members.get(k))) {
                    return false;
                }
            }
            return true;
        }
        if (a instanceof JsonArray && b instanceof JsonArray) {
            JsonArray aa = (JsonArray) a;
            JsonArray ab = (JsonArray) b;
            if (aa.items.size() != ab.items.size()) {
                return false;
            }
            for (int i = 0; i < aa.items.size(); i++) {
                if (!jsonEquals(aa.items.get(i), ab.items.get(i))) {
                    return false;
                }
            }
            return true;
        }
        if (a instanceof JsonNumber && b instanceof JsonNumber) {
            return ((JsonNumber) a).asText().equals(((JsonNumber) b).asText());
        }
        if (a instanceof JsonString && b instanceof JsonString) {
            return ((JsonString) a).value.equals(((JsonString) b).value);
        }
        if (a instanceof JsonBool && b instanceof JsonBool) {
            return ((JsonBool) a).value == ((JsonBool) b).value;
        }
        return (a instanceof JsonNull) && (b instanceof JsonNull);
    }

    // ------------------------------------------------------------------
    // Driver
    // ------------------------------------------------------------------

    static int parsed = 0;
    static int rejected = 0;

    static JsonValue parseOrThrow(String src) {
        Parser p = new Parser(src);
        JsonValue v = p.parseValue();
        p.skipWs();
        if (p.pos < p.src.length()) {
            throw new JsonParseException("trailing garbage", p.pos);
        }
        return v;
    }

    static void roundTrip(String src) {
        JsonValue v = parseOrThrow(src);
        StringBuilder compact = new StringBuilder();
        v.serialize(compact);
        JsonValue again = parseOrThrow(compact.toString());
        if (!jsonEquals(v, again)) {
            throw new IllegalStateException("round-trip mismatch for: " + src);
        }
        parsed++;
        StringBuilder pretty = new StringBuilder();
        prettyPrint(pretty, v, 0);
        System.out.println("ok   " + compact.toString());
        System.out.println("pretty lines: " + countLines(pretty.toString()));
    }

    static int countLines(String s) {
        int n = 1;
        for (int i = 0; i < s.length(); i++) {
            if (s.charAt(i) == '\n') {
                n++;
            }
        }
        return n;
    }

    static void expectRejected(String src, String wantPrefix) {
        try {
            parseOrThrow(src);
            throw new IllegalStateException("expected rejection for: " + src);
        } catch (JsonParseException e) {
            String msg = e.getMessage();
            if (!msg.startsWith(wantPrefix)) {
                throw new IllegalStateException(
                    "wrong rejection message for [" + src + "]: " + msg);
            }
            System.out.println("rej  " + msg);
            rejected++;
        }
    }

    public static void main(String[] args) {
        // The backslash char, built arithmetically so Java's pre-lexing
        // unicode-escape processing can never eat a source-level uXXXX.
        char BS = (char) 92;

        roundTrip("null");
        roundTrip("true");
        roundTrip("false");
        roundTrip("42");
        roundTrip("-17");
        roundTrip("3.5");
        roundTrip("1.25e3");
        roundTrip("\"plain\"");
        // JSON text: "escaped \" quoted \n text"  (escapes inside the string)
        roundTrip("\"escaped \\\" quoted " + BS + "n text\"");
        roundTrip("[]");
        roundTrip("[1,2,3]");
        roundTrip("{}");
        roundTrip("{\"a\":1,\"b\":[true,null,\"x\"],\"c\":{\"d\":-2.5}}");

        // Deep nesting: 50 levels of arrays, built programmatically.
        StringBuilder deep = new StringBuilder();
        for (int i = 0; i < 50; i++) {
            deep.append('[');
        }
        deep.append("7");
        for (int i = 0; i < 50; i++) {
            deep.append(']');
        }
        roundTrip(deep.toString());

        // Wide object: unicode escapes (A, e-acute, CJK ideograph) built via
        // char concat; control escapes; quotes and a backslash in a value.
        String wide = "{\"u\":\"" + BS + "u0041" + BS + "u00e9" + BS + "u4e2d\","
            + "\"ctl\":\"a" + BS + "tb" + BS + "nc\","
            + "\"s\":\"" + BS + "\"q" + BS + "\"" + BS + BS + "\"}";
        roundTrip(wide);

        // The corpus document: a small config file, parsed and pretty-printed.
        String corpus = "{\"name\":\"j2r\",\"version\":3,\"flags\":[true,false,null],"
            + "\"deps\":[{\"n\":\"core\",\"v\":\"1.0\"},{\"n\":\"rt\",\"v\":\"2.3.1\"}],"
            + "\"tuning\":{\"threshold\":0.75,\"cache\":1024}}";
        JsonValue doc = parseOrThrow(corpus);
        StringBuilder pretty = new StringBuilder();
        prettyPrint(pretty, doc, 0);
        System.out.println(pretty.toString());

        // Structural queries over the parsed corpus.
        JsonObject root = (JsonObject) doc;
        System.out.println("name=" + ((JsonString) root.get("name")).value);
        System.out.println("version=" + ((JsonNumber) root.get("version")).raw);
        JsonArray deps = (JsonArray) root.get("deps");
        System.out.println("deps=" + deps.items.size());
        for (int i = 0; i < deps.items.size(); i++) {
            JsonObject d = (JsonObject) deps.items.get(i);
            System.out.println("dep[" + i + "]=" + ((JsonString) d.get("n")).value
                + "@" + ((JsonString) d.get("v")).value);
        }
        JsonObject tuning = (JsonObject) root.get("tuning");
        System.out.println("threshold=" + ((JsonNumber) tuning.get("threshold")).asText());
        System.out.println("cache=" + ((JsonNumber) tuning.get("cache")).asText());

        // Unicode payload round-trips: chars > 127 embedded directly.
        JsonValue uni = parseOrThrow("\"é中ß\"");
        StringBuilder uniOut = new StringBuilder();
        uni.serialize(uniOut);
        System.out.println("uni=" + uniOut.toString());

        // Malformed inputs: the exact rejection messages are part of the
        // transcript (deterministic, JVM-diffed).
        expectRejected("", "unexpected end of input");
        expectRejected("{", "unexpected end of input");
        expectRejected("{\"a\":}", "unexpected character '}'");
        expectRejected("[1,2,", "unexpected end of input");
        expectRejected("[1 2]", "expected ',' or ']' but got '2'");
        expectRejected("{\"a\" 1}", "expected ':' but got '1'");
        expectRejected("tru", "invalid literal, wanted true");
        expectRejected("\"" + BS + "q\"",
            "bad escape '" + BS + "q'");
        expectRejected("12abc", "trailing garbage");
        expectRejected("[", "unexpected end of input");
        expectRejected("{\"a\":1", "unexpected end of input");
        expectRejected("{\"a\":1,}}", "expected '\"' but got '}'");
        expectRejected("\"" + BS + "u00zz\"",
            "bad hex digit 'z'");

        System.out.println("parsed=" + parsed + " rejected=" + rejected);
        System.out.println("BigJson done");
    }
}
