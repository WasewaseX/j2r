package chain;

public class C10 {
    public C11 next = new C11();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C10";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C10";
    }
}
