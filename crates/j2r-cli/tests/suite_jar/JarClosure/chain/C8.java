package chain;

public class C8 {
    public C9 next = new C9();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C8";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C8";
    }
}
