package chain;

public class C7 {
    public C8 next = new C8();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C7";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C7";
    }
}
