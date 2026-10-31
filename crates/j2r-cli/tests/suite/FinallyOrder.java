public class FinallyOrder {
    static int plain() {
        try {
            return 1;
        } finally {
            System.out.println("finally-plain");
        }
    }
    static int withException() {
        try {
            throw new RuntimeException("inner");
        } catch (RuntimeException e) {
            System.out.println("caught: " + e.getMessage());
            return 2;
        } finally {
            System.out.println("finally-exception");
        }
    }
    static int rethrow() {
        try {
            try {
                throw new IllegalStateException("deep");
            } finally {
                System.out.println("inner-finally");
            }
        } catch (IllegalStateException e) {
            System.out.println("outer-caught");
            return 3;
        }
    }
    public static void main(String[] args) {
        System.out.println(plain());
        System.out.println(withException());
        System.out.println(rethrow());
        try {
            try {
                throw new RuntimeException("a");
            } finally {
                try {
                    throw new RuntimeException("b");
                } catch (RuntimeException e) {
                    System.out.println("nested-finally-caught");
                }
            }
        } catch (RuntimeException e) {
            System.out.println("outer got: " + e.getMessage());
        }
    }
}
