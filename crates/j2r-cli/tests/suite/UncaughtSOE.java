public class UncaughtSOE {
    static int down(int n) {
        return down(n + 1) + 1;
    }
    public static void main(String[] args) {
        down(1);
        System.out.println("unreachable");
    }
}
