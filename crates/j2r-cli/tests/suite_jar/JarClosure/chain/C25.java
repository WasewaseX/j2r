package chain;

public class C25 {
    public C26 next = new C26();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C25";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C25";
    }
}
