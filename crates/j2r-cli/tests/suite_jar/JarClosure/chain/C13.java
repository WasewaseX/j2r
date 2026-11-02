package chain;

public class C13 {
    public C14 next = new C14();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C13";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C13";
    }
}
