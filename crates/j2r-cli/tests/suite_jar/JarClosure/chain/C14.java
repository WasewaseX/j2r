package chain;

public class C14 {
    public C15 next = new C15();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C14";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C14";
    }
}
