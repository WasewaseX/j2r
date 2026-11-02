package chain;

public class C2 {
    public C3 next = new C3();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C2";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C2";
    }
}
