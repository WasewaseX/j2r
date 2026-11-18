/**
 * Differential driver over the minimal-json library (real-world corpus).
 * Deterministic output only; every println is stable across JVMs.
 *
 * scanaislop round-4 wave: the driver imports the minimaljsonentry entry
 * package (the amalgam shape retires); the driver body is unchanged.
 */
import minimaljsonentry.*;

public class MinimalJsonCorpus {
  public static void main(String[] args) {
    String src = "{\"name\":\"j2r\",\"wave\":3,\"tags\":[\"a\",\"b\\\"c\"],"
        + "\"nested\":{\"pi\":3.14,\"on\":true,\"off\":false,\"nil\":null},\"empty\":{}}";
    JsonValue v = Json.parse(src);
    System.out.println(v.toString(WriterConfig.PRETTY_PRINT));
    JsonObject o = new JsonObject();
    o.add("k", 42).add("s", "esc\"\t\n<>").add("arr", new JsonArray().add(1).add(2.5).add(false));
    System.out.println(o.toString());
    System.out.println("roundtrip=" + Json.parse(o.toString()).toString().equals(o.toString()));
    System.out.println("name=" + v.asObject().get("name").asString());
    System.out.println("pi=" + v.asObject().get("nested").asObject().get("pi").asDouble());
    System.out.println("tags=" + v.asObject().get("tags").asArray().size());
    try {
      Json.parse("{bad}");
    } catch (ParseException e) {
      System.out.println("err@" + e.getLocation().line + ":" + e.getLocation().column
          + " " + e.getMessage());
    }
    System.out.println(new JsonNumber("12345678901234567890").toString());
    System.out.println("literal=" + Json.value(true).toString() + "," + Json.value(null).toString());
    JsonArray nums = new JsonArray().add(0).add(-1).add(0.5).add(1e10);
    System.out.println("sum=" + (nums.get(0).asInt() + nums.get(1).asInt())
        + " half=" + nums.get(2).asDouble() + " big=" + nums.get(3).toString());
  }
}
