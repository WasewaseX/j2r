import java.util.Objects;

public class ObjectsNPE {
    public static void main(String[] args) {
        String ok = "value";
        System.out.println(Objects.requireNonNull(ok));
        System.out.println(Objects.isNull(null));
        System.out.println(Objects.isNull(ok));
        System.out.println(Objects.nonNull(null));
        System.out.println(Objects.nonNull(ok));
        System.out.println(Objects.toString(null));
        System.out.println(Objects.toString(ok));
        System.out.println(Objects.hashCode(null));
        System.out.println(Objects.hashCode("abc"));
        System.out.println(Objects.equals("a", "a"));
        System.out.println(Objects.equals("a", "b"));
        System.out.println(Objects.equals(null, null));
        System.out.println(Objects.equals(null, "a"));
        System.out.println(Objects.equals("a", null));
        try {
            Objects.requireNonNull(null);
        } catch (NullPointerException e) {
            System.out.println("npe1:" + e.getMessage());
        }
        try {
            Objects.requireNonNull(null, "must not be null");
        } catch (NullPointerException e) {
            System.out.println("npe2:" + e.getMessage());
        }
        try {
            Objects.requireNonNull(null, "second");
            System.out.println("unreachable");
        } catch (NullPointerException e) {
            System.out.println("caught:" + e.getMessage());
        }
    }
}
