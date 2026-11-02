package chain;

public class C5 {
    public C6 next = new C6();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C5";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C5";
    }
}
