package chain;

public class C22 {
    public C23 next = new C23();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C22";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C22";
    }
}
