public class JaggedNew {
    public static void main(String[] args) {
        int[][] a = new int[3][];
        System.out.println(a.length);
        System.out.println(a[0] == null);
        a[0] = new int[2];
        a[1] = new int[4];
        a[0][1] = 42;
        System.out.println(a[0][1]);
        int total = 0;
        for (int[] row : a) {
            if (row == null) { total += 100; continue; }
            for (int v : row) total += v;
        }
        System.out.println(total);
        String[][] s = new String[2][];
        s[0] = new String[1];
        s[0][0] = "hi";
        System.out.println(s[0][0]);
        Object[] holder = a;       // int[][] is an Object
        int[][] back = (int[][]) holder;
        System.out.println(back[0].length);
    }
}
