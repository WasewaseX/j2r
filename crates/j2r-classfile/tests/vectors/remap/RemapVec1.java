public class RemapVec1 {
  private static int counter;
  public final long stamp = 123456789012345L;
  protected volatile transient short mask = 7;
  byte tiny = 3;
  char glyph = 'Z';
  float ratio = 0.5f;
  double precise = 3.14159;
  boolean ok = true;
  String label = "vec1";
  static final String VERSION = "t15.1-vector-1";
  int[] cells = new int[8];
  public static void main(String[] args) {
    RemapVec1 v = new RemapVec1();
    counter += 1;
    long x = v.stamp + counter;
    double d = v.precise * 2.0 + v.ratio;
    char c = (char)(v.glyph + 1);
    boolean flag = v.ok && (v.mask > 0);
    String msg = "counter=" + counter + " stamp=" + x + " d=" + d + " c=" + c + " ok=" + flag + " v=" + VERSION;
    System.out.println(msg);
    for (int i = 0; i < v.cells.length; i++) v.cells[i] = i * i;
    int sum = 0;
    for (int q : v.cells) sum += q;
    System.out.println("sum=" + sum + " len=" + v.cells.length + " label=" + v.label);
  }
}
