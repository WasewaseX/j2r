package chain;

public class C12 {
    public C13 next = new C13();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C12";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C12";
    }
}
