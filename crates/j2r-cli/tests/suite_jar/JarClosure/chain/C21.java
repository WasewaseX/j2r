package chain;

public class C21 {
    public C22 next = new C22();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C21";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C21";
    }
}
