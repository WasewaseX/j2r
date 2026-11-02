/**
 * FieldDefaults — pins JLS §4.12.5 instance-field default initialization.
 *
 * Found by the BigJson real-program fixture: a field the constructor never
 * assigns must read back as the JVM default (0 / false / '\u0000' / null),
 * not as an uninitialized slot. Covers every primitive kind, reference
 * fields, inherited fields from a user superclass, and fields assigned
 * only on one constructor path.
 */
public class FieldDefaults {

    static class Defaults {
        int i;
        long l;
        double d;
        float f;
        boolean z;
        char c;
        byte b;
        short s;
        String ref;
        int[] arr;
        Object o;

        int lateAssigned;
    }

    static class SubDefaults extends Defaults {
        int subField;
        String subRef;

        SubDefaults(int assignSome) {
            // Assigns only SOME inherited fields; the rest must still read
            // as JVM defaults.
            this.i = assignSome;
            this.ref = "set-in-sub";
        }
    }

    static class OnlyDefaultCtor {
        long untouched;
        String alsoUntouched;
    }

    public static void main(String[] args) {
        Defaults d = new Defaults();
        System.out.println("i=" + d.i);
        System.out.println("l=" + d.l);
        System.out.println("d=" + d.d);
        System.out.println("f=" + d.f);
        System.out.println("z=" + d.z);
        System.out.println("c=" + (int) d.c);
        System.out.println("b=" + d.b);
        System.out.println("s=" + d.s);
        System.out.println("ref=" + d.ref);
        System.out.println("arr=" + (d.arr == null ? "null" : "non-null"));
        System.out.println("o=" + d.o);
        System.out.println("late=" + d.lateAssigned);

        // Increment a never-assigned field: reads must be 0-based, and the
        // slot must behave like an int afterwards (this is the exact shape
        // that panicked before the fix: iadd over a Void slot).
        d.lateAssigned += 5;
        d.lateAssigned++;
        System.out.println("late=" + d.lateAssigned);

        // Inherited defaults through a user superclass.
        SubDefaults sub = new SubDefaults(7);
        System.out.println("sub.i=" + sub.i);
        System.out.println("sub.l=" + sub.l);
        System.out.println("sub.z=" + sub.z);
        System.out.println("sub.ref=" + sub.ref);
        System.out.println("sub.arr=" + (sub.arr == null ? "null" : "non-null"));
        System.out.println("sub.subField=" + sub.subField);
        System.out.println("sub.subRef=" + sub.subRef);

        // A class whose constructor assigns nothing at all.
        OnlyDefaultCtor odc = new OnlyDefaultCtor();
        System.out.println("odc.untouched=" + odc.untouched);
        System.out.println("odc.alsoUntouched=" + odc.alsoUntouched);

        // Fresh object identity: two instances' defaults are independent.
        Defaults d2 = new Defaults();
        d.lateAssigned = 100;
        System.out.println("independent=" + (d2.lateAssigned == 0 && d.lateAssigned == 100));

        System.out.println("FieldDefaults done");
    }
}
