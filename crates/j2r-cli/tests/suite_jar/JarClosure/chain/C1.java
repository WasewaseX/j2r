package chain;

public class C1 {
    public C2 next = new C2();

    public String walk(int depth) {
        if (depth == 0) {
            return "at-C1";
        }
        return next.walk(depth - 1);
    }

    public String label() {
        return "C1";
    }

    public static void main(String[] args) {
        C1 first = new C1();
        // One `new C1()` eagerly constructs the whole 30-node chain:
        // every C<i> (i < 30) news its successor in a field initializer,
        // so the closure loader must pull every link from the jar even
        // though only C1 is named on the command line.
        System.out.println(first.walk(29));
        System.out.println(first.next.next.label());
        System.out.println(first.next.label());
    }
}
