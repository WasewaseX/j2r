// Audit A2 fixture: happy-path try-with-resources with three resources —
// javac nests three TWR blocks; close() must run LIFO (C, B, A) with no
// suppression involved. All output lands on stdout, exit 0.
public class TwrThree {
    static class Res implements AutoCloseable {
        private final String name;
        Res(String name) {
            this.name = name;
            System.out.println("acquire " + name);
        }
        void use(int units) {
            System.out.println("use " + name + " " + units);
        }
        @Override
        public void close() {
            System.out.println("release " + name);
        }
    }

    public static void main(String[] args) {
        try (Res a = new Res("A"); Res b = new Res("B"); Res c = new Res("C")) {
            a.use(3);
            b.use(5);
            c.use(8);
        }
        System.out.println("done");
    }
}
