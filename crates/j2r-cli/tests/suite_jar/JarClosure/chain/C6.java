package chain;

public class C6 {
    public C7 next = new C7();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C6";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C6";
    }
}
