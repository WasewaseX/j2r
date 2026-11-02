package chain;

public class C28 {
    public C29 next = new C29();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C28";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C28";
    }
}
