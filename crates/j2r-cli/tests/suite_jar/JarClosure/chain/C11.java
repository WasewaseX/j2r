package chain;

public class C11 {
    public C12 next = new C12();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C11";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C11";
    }
}
