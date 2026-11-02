package c;

import a.b.Util;

public class Order {
    static {
        System.out.println("clinit-order");
    }

    public static String describe() {
        return "order(" + Util.scale(5) + ")";
    }
}
