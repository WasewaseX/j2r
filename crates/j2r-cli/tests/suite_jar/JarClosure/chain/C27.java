package chain;

public class C27 {
    public C28 next = new C28();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C27";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C27";
    }
}
