import java.lang.reflect.Constructor;

public class NewProbe2 {
    static class Errant {
        static {
            if (System.currentTimeMillis() > 0) throw new RuntimeException("boom");
        }
        Errant() {
            System.out.println("Errant<init>");
        }
    }

    static class CtorThrows {
        CtorThrows() {
            if (System.currentTimeMillis() > 0) throw new RuntimeException("ctor-boom");
        }
    }

    public static void main(String[] args) throws Exception {
        // P7: newInstance as the FIRST access of an errant-clinit class.
        Constructor<?> k = Class.forName("NewProbe2$Errant", false, NewProbe2.class.getClassLoader())
                .getDeclaredConstructor();
        k.setAccessible(true);
        try {
            k.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println("cause: " + t.getCause());
        }
        // P8: the second newInstance on the errant class.
        try {
            k.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println("cause: " + t.getCause());
        }
        // P9: a ctor that itself throws — the ITE wrap.
        Constructor<?> k2 = Class.forName("NewProbe2$CtorThrows").getDeclaredConstructor();
        k2.setAccessible(true);
        try {
            k2.newInstance();
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println("cause: " + t.getCause());
        }
    }
}
