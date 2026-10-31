public class MutualRecursion {
    static boolean isEven(int n) {
        if (n == 0) return true;
        return isOdd(n - 1);
    }
    static boolean isOdd(int n) {
        if (n == 0) return false;
        return isEven(n - 1);
    }
    static long ack(int m, int n) {
        if (m == 0) return n + 1;
        if (n == 0) return ack(m - 1, 1);
        return ack(m - 1, (int) ack(m, n - 1));
    }
    static long collatzSteps(long n, long depth) {
        if (n == 1) return depth;
        if (n % 2 == 0) return collatzSteps(n / 2, depth + 1);
        return collatzSteps(3 * n + 1, depth + 1);
    }
    public static void main(String[] args) {
        System.out.println(isEven(10));
        System.out.println(isOdd(10));
        System.out.println(isEven(7));
        System.out.println(ack(2, 3));
        System.out.println(ack(3, 3));
        System.out.println(collatzSteps(27, 0));
    }
}
