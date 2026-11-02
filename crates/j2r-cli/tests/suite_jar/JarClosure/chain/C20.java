package chain;

public class C20 {
    public C21 next = new C21();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C20";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C20";
    }
}
