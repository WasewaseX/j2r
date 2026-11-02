package chain;

public class C15 {
    public C16 next = new C16();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C15";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C15";
    }
}
