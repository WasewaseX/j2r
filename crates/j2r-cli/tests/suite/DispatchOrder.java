public class DispatchOrder {
    static class Base {
        int baseField = 1;
        Base() {
            System.out.println("Base ctor: vf()=" + vf());
            baseField = 10;
        }
        String vf() { return "Base.vf"; }
        static String sf() { return "Base.sf"; }
    }
    static class Sub extends Base {
        int subField = 2;
        Sub() {
            System.out.println("Sub ctor body: base=" + baseField + " sub=" + subField);
        }
        @Override
        String vf() { return "Sub.vf"; }
        static String sf() { return "Sub.sf"; }
    }
    static class InitOrder {
        int a = trace("field-a", 1);
        { System.out.println("instance-init-1"); }
        int b = trace("field-b", 2);
        InitOrder() {
            System.out.println("ctor: a=" + a + " b=" + b);
        }
        static int trace(String tag, int v) {
            System.out.println("init " + tag);
            return v;
        }
    }
    public static void main(String[] args) {
        Base asBase = new Sub();
        System.out.println(asBase.vf());
        System.out.println(Base.sf());
        System.out.println(Sub.sf());
        new InitOrder();
    }
}
