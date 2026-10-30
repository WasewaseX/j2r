public class Recursion {
    static long fib(int n) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }

    static long fact(int n) {
        return n <= 1 ? 1 : n * fact(n - 1);
    }

    static boolean isEven(int n) {
        if (n == 0) return true;
        return isOdd(n - 1);
    }

    static boolean isOdd(int n) {
        if (n == 0) return false;
        return isEven(n - 1);
    }

    static int binarySearch(int[] sorted, int target) {
        int lo = 0;
        int hi = sorted.length - 1;
        while (lo <= hi) {
            int mid = (lo + hi) / 2;
            int v = sorted[mid];
            if (v == target) return mid;
            if (v < target) lo = mid + 1;
            else hi = mid - 1;
        }
        return -1;
    }

    static int ack(int m, int n) {
        if (m == 0) return n + 1;
        if (n == 0) return ack(m - 1, 1);
        return ack(m - 1, ack(m, n - 1));
    }

    public static void main(String[] args) {
        System.out.println(fib(20));
        System.out.println(fact(12));
        System.out.println(isEven(10));
        System.out.println(isEven(7));
        int[] sorted = {1, 3, 5, 7, 9, 11, 13};
        System.out.println(binarySearch(sorted, 7));
        System.out.println(binarySearch(sorted, 8));
        System.out.println(binarySearch(sorted, 1));
        System.out.println(binarySearch(sorted, 13));
        System.out.println(ack(2, 3));
    }
}
