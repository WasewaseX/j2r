package chain;

public class C16 {
    public C17 next = new C17();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C16";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C16";
    }
}
