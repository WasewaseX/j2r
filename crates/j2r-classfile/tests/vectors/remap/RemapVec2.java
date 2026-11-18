import java.util.List;
import java.util.Map;
import java.util.ArrayList;
public class RemapVec2 {
  private List<Map<String, Integer>> ledger = new ArrayList<>();
  public int score(String who) throws IllegalStateException {
    try {
      int n = who.length();
      if (n == 0) throw new IllegalArgumentException("empty");
      Map<String, Integer> row = Map.of(who, n);
      ledger.add(row);
      synchronized (ledger) { n += ledger.size(); }
      return n;
    } catch (IllegalStateException e) {
      throw e;
    } catch (RuntimeException e) {
      return -1;
    } finally {
      System.out.println("scored");
    }
  }
  static class Inner {
    final String tag;
    Inner(String t) { this.tag = t; }
  }
  interface Shape { double area(); }
  static class Box implements Shape {
    public double area() { return 42.0; }
  }
}
