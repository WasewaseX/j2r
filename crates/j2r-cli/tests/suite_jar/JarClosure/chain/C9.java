package chain;

public class C9 {
    public C10 next = new C10();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C9";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C9";
    }
}
