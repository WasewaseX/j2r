package chain;

public class C4 {
    public C5 next = new C5();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C4";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C4";
    }
}
