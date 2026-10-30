public class Arrays {
    public static void main(String[] args) {
        int[] xs = new int[5];
        for (int i = 0; i < xs.length; i++) xs[i] = i * i;
        int total = 0;
        for (int v : xs) total += v;
        System.out.println(total);
        System.out.println(xs[3]);
        System.out.println(xs.length);
        String[] names = {"alpha", "beta", "gamma"};
        for (String n : names) System.out.println(n);
        String[] dyn = new String[2];
        System.out.println(dyn[0]);
        dyn[0] = "set";
        System.out.println(dyn[0]);
        long[] ls = new long[3];
        ls[1] = 5000000000L;
        System.out.println(ls[1] + ls[1]);
        double[] ds = {1.5, 2.5};
        System.out.println(ds[0] + ds[1]);
        char[] cs = new char[2];
        cs[0] = 'A';
        System.out.println((int) cs[0]);
        System.out.println(cs[1] == 0);
        boolean[] bs = new boolean[1];
        System.out.println(bs[0]);
        // 2D via anewarray + loop
        int[][] grid = new int[3][];
        for (int r = 0; r < 3; r++) {
            grid[r] = new int[r + 1];
            for (int c = 0; c <= r; c++) grid[r][c] = r * 10 + c;
        }
        int sum = 0;
        for (int[] row : grid) {
            for (int v : row) {
                sum += v;
                System.out.print(v);
                System.out.print(" ");
            }
        }
        System.out.println();
        System.out.println(sum);
        Object[] mixed = new Object[3];
        mixed[0] = "str";
        mixed[1] = Integer.valueOf(5);
        mixed[2] = null;
        System.out.println(mixed[0]);
        System.out.println(mixed[1]);
        System.out.println(mixed[2]);
        try {
            System.out.println(xs[99]);
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println("caught: " + e.getMessage());
        }
        try {
            int[] neg = new int[-2];
            System.out.println(neg.length);
        } catch (NegativeArraySizeException e) {
            System.out.println("negative!");
        }
    }
}
