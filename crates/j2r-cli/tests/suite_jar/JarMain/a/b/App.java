package a.b;

import c.Order;

public class App {
    public static void main(String[] args) {
        System.out.println(Util.BOOT);
        System.out.println(Order.describe());
        System.out.println(Util.scale(2));
        System.out.println(Counter.next());
    }
}
