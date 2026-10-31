public class ErrStream {
    public static void main(String[] args) {
        System.out.println("out-1");
        System.err.println("err-1: " + 42);
        System.out.println("out-2");
        System.err.print("no-newline");
        System.err.println();
        System.err.println(255);
        System.err.println(1.5);
        System.err.println('c');
        System.err.println(true);
        System.err.println(args.length);
        System.out.println("out-3");
    }
}
