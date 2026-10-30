class Config {
    static final int MAX = 100;
    static final String NAME = "j2r";
    static final long BIG = 123456789L;
    static int count = 5;
    static int[] data = new int[MAX / 10];

    static {
        for (int i = 0; i < data.length; i++) data[i] = i + 1;
        count += 10;
    }

    static int bump() {
        count++;
        return count;
    }
}

class Helper {
    static String tag() {
        return Config.NAME + "-" + Config.count;
    }
}

public class Statics {
    public static void main(String[] args) {
        System.out.println(Config.MAX);
        System.out.println(Config.NAME);
        System.out.println(Config.BIG);
        System.out.println(Config.count);
        System.out.println(Config.data.length);
        int sum = 0;
        for (int v : Config.data) sum += v;
        System.out.println(sum);
        Config.bump();
        Config.bump();
        System.out.println(Config.count);
        System.out.println(Helper.tag());
    }
}
