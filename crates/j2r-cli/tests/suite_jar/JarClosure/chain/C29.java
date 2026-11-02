package chain;

public class C29 {
    public C30 next = new C30();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C29";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C29";
    }
}
