public class LabeledLoop {
    public static void main(String[] args) {
        outer:
        for (int i = 0; i < 4; i++) {
            for (int j = 0; j < 4; j++) {
                if (i == 2 && j == 1) continue outer;
                if (i == 3 && j == 2) break outer;
                System.out.print(i + "" + j + " ");
            }
            System.out.println("|");
        }
        System.out.println();
        int n = 0;
        search:
        while (n < 10) {
            n++;
            int m = 0;
            do {
                m++;
                if (m * n > 6) continue search;
            } while (m < 10);
        }
        System.out.println(n);
    }
}
