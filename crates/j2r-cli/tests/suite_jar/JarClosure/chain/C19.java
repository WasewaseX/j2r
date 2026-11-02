package chain;

public class C19 {
    public C20 next = new C20();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C19";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C19";
    }
}
