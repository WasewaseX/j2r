public class RemapVec3 {
  record Pair(int k, String v) {}
  enum Tone { HI, LO }
  public static void main(String[] args) {
    Pair p = new Pair(1, "one");
    String nul = "a\u0000b";
    String exotic = "\ud834\udd1e";
    Runnable r = () -> System.out.println("lambda-run");
    r.run();
    Tone t = Tone.HI;
    String s = switch (t) { case HI -> "high"; case LO -> "low"; };
    System.out.println("p=" + p + " nul.len=" + nul.length() + " exotic=" + exotic + " s=" + s + " cmp=" + nul.compareTo("ab"));
  }
}
