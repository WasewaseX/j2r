package chain;

public class C24 {
    public C25 next = new C25();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C24";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C24";
    }
}
