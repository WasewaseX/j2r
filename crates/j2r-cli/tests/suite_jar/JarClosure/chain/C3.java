package chain;

public class C3 {
    public C4 next = new C4();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C3";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C3";
    }
}
