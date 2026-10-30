public class Control {
    static String grade(int score) {
        if (score >= 90) return "A";
        else if (score >= 80) return "B";
        else if (score >= 70) return "C";
        else return "F";
    }
    public static void main(String[] args) {
        System.out.println(grade(95));
        System.out.println(grade(84));
        System.out.println(grade(71));
        System.out.println(grade(3));
        int x = 7;
        String parity = x % 2 == 0 ? "even" : "odd";
        System.out.println(parity);
        int sum = 0;
        for (int i = 1; i <= 10; i++) sum += i;
        System.out.println(sum);
        int n = 40;
        while (n % 3 != 0) n++;
        System.out.println(n);
        do { n--; } while (n % 3 != 0);
        System.out.println(n);
        outer:
        for (int i = 0; i < 5; i++) {
            for (int j = 0; j < 5; j++) {
                if (i * j > 6) break outer;
                if (j == 2) continue;
                System.out.print(i * 10 + j);
                System.out.print(" ");
            }
        }
        System.out.println();
        for (int i = 0; i < 12; i++) {
            switch (i % 4) {
                case 0: System.out.print("zero "); break;
                case 1: System.out.print("one "); break;
                case 2:
                case 3: System.out.print("many "); break;
            }
        }
        System.out.println();
        String[] names = {"one", "two", "three", "four"};
        for (String s : names) {
            switch (s) {
                case "one": System.out.println("1"); break;
                case "two": System.out.println("2"); break;
                default: System.out.println("? " + s);
            }
        }
        int big = 1000;
        switch (big) {
            case 10: System.out.println("ten");
            case 100: System.out.println("hundred");
            case 1000: System.out.println("thousand");
            default: System.out.println("done");
        }
    }
}
