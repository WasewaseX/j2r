package chain;

public class C17 {
    public C18 next = new C18();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C17";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C17";
    }
}
