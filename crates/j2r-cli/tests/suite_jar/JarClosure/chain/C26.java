package chain;

public class C26 {
    public C27 next = new C27();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C26";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C26";
    }
}
