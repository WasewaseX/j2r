package chain;

public class C23 {
    public C24 next = new C24();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C23";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C23";
    }
}
