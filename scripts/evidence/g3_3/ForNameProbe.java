public class ForNameProbe {
    static class Init {
        static {
            System.out.println("Init<clinit>");
        }
    }

    static class Err {
        static {
            System.out.println("Err<clinit>");
            if (System.currentTimeMillis() > 0) throw new RuntimeException("boom");
        }
    }

    static class ErrA {
        static {
            System.out.println("ErrA<clinit>");
            if (System.currentTimeMillis() > 0) throw new AssertionError("bang");
        }
    }

    public static void main(String[] a) throws Exception {
        // F1: forName(name) RUNS the clinit of a never-touched class.
        Class.forName("ForNameProbe$Init");
        System.out.println("--");
        // F2: forName(name, false, loader) does NOT run it.
        Class.forName("ForNameProbe$Init", false, ForNameProbe.class.getClassLoader());
        System.out.println("no-init ok");
        // F3: a throwing clinit surfaces the EIIIE law through forName.
        try {
            Class.forName("ForNameProbe$Err");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // F4: the errant NCDFE on the second forName (+ its cause).
        try {
            Class.forName("ForNameProbe$Err");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
            System.out.println(t.getCause());
        }
        // F5: an Error clinit rethrows AS-IS.
        try {
            Class.forName("ForNameProbe$ErrA");
            System.out.println("NO THROW?!");
        } catch (Throwable t) {
            System.out.println(t);
        }
        // F6: the array descriptor resolves WITHOUT initializing the
        // component (JLS 12.4.1).
        Class<?> arr = Class.forName("[LForNameProbe$Init2;");
        System.out.println("array resolved: " + arr.getName());
    }

    static class Init2 {
        static {
            System.out.println("Init2<clinit>");
        }
    }
}
