package chain;

public class C18 {
    public C19 next = new C19();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C18";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C18";
    }
}
