public class Arrays2D {
    public static void main(String[] args) {
        int[][] grid = new int[3][4];
        for (int r = 0; r < grid.length; r++) {
            for (int c = 0; c < grid[r].length; c++) {
                grid[r][c] = r * 10 + c;
            }
        }
        int sum = 0;
        for (int r = 0; r < grid.length; r++) {
            for (int c = 0; c < grid[r].length; c++) {
                sum += grid[r][c];
            }
        }
        System.out.println(sum);
        System.out.println(grid.length);
        System.out.println(grid[0].length);
        // jagged
        int[][] jag = new int[3][];
        jag[0] = new int[1];
        jag[1] = new int[2];
        jag[2] = new int[3];
        for (int r = 0; r < jag.length; r++) {
            for (int c = 0; c < jag[r].length; c++) {
                jag[r][c] = (r + 1) * (c + 1);
            }
        }
        for (int r = 0; r < jag.length; r++) {
            for (int c = 0; c < jag[r].length; c++) {
                System.out.print(jag[r][c] + (c == jag[r].length - 1 ? "" : ","));
            }
            System.out.println();
        }
        // literal 2D
        int[][] lit = { {1, 2}, {3, 4, 5} };
        System.out.println(lit[1][2]);
        System.out.println(lit[0][1] + lit[1][0]);
        // String matrix
        String[][] names = { { "a", "b" }, { "c" } };
        System.out.println(names[0][1] + names[1][0]);
        // long 2D default value
        long[][] lm = new long[2][2];
        System.out.println(lm[1][1]);
    }
}
