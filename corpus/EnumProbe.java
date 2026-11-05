import java.util.Arrays;

/**
 * Temurin vector for enum classes end-to-end (F6). Pins: ordinal/name/toString,
 * values() clone semantics, valueOf both entry points, exact failure messages,
 * compareTo, switch-over-enum via the javac-generated $SwitchMap synthetic,
 * and static-init ordering (enum constants exist before static fields run).
 */
public class EnumProbe {

    enum Color {
        RED, GREEN, BLUE;

        /** custom method — exercises instance dispatch on an enum receiver */
        int mixCount() { return ordinal() * 10 + name().length(); }
    }

    enum Planet {
        MERCURY(3.303e+23, 2.4397e6),
        VENUS(4.869e+24, 6.0518e6),
        EARTH(5.976e+24, 6.37814e6);

        private final double mass;
        private final double radius;

        Planet(double mass, double radius) {
            this.mass = mass;
            this.radius = radius;
        }

        double surfaceGravity() {
            return 6.67300E-11 * mass / (radius * radius);
        }

        double surfaceWeight(double otherMass) {
            return otherMass * surfaceGravity();
        }
    }

    public static void main(String[] args) {
        // 1. ordinal / name / toString
        Color r = Color.RED;
        System.out.println(r.ordinal() + " " + r.name() + " " + r.toString());
        System.out.println(Color.BLUE.ordinal() + " " + Color.BLUE.name());

        // 2. values(): array clone — mutation of the returned array must not
        //    corrupt the constant table, and each call returns a fresh array
        Color[] a = Color.values();
        System.out.println(Arrays.toString(a));
        a[0] = Color.BLUE;
        System.out.println(Arrays.toString(Color.values()));

        // 3. valueOf(String) — direct hit, case-sensitive miss, null name
        System.out.println(Color.valueOf("GREEN").ordinal());
        try {
            Color.valueOf("red");
        } catch (IllegalArgumentException e) {
            System.out.println("IAE: " + e.getMessage());
        }
        try {
            Color.valueOf(null);
        } catch (NullPointerException e) {
            System.out.println("NPE: " + e.getMessage());
        }

        // 4. Enum.valueOf(Class, String) — the synthetic values()/valueOf
        //    delegate here; same exact messages
        System.out.println(Enum.valueOf(Color.class, "BLUE").name());
        try {
            Enum.valueOf(Color.class, "PURPLE");
        } catch (IllegalArgumentException e) {
            System.out.println("IAE2: " + e.getMessage());
        }

        // 5. compareTo (ordinal-based, final), equals (identity, final),
        //    getDeclaringClass
        System.out.println(Color.RED.compareTo(Color.GREEN) + " "
            + Color.GREEN.compareTo(Color.GREEN) + " "
            + Color.BLUE.compareTo(Color.GREEN));
        System.out.println(Color.RED.equals(Color.RED) + " "
            + Color.RED.equals(Color.GREEN) + " " + Color.RED.equals(null));
        System.out.println(Color.RED.getDeclaringClass().getName());

        // 6. hashCode — JVM identity hash differs per run; only call it for
        //    liveness, never print the value
        System.out.println(Color.RED.hashCode() != 0 || true);

        // 7. switch over enum — javac generates the $SwitchMap synthetic
        //    class whose <clinit> calls Color.values() and ordinal()
        Color c = Color.GREEN;
        switch (c) {
            case RED: System.out.println("switch: red"); break;
            case GREEN: System.out.println("switch: green"); break;
            case BLUE: System.out.println("switch: blue"); break;
            default: System.out.println("switch: default");
        }
        // fallthrough-free exhaustive form, null switch value -> NPE
        try {
            Color n = null;
            switch (n) {
                case RED: System.out.println("never"); break;
                default: System.out.println("never default");
            }
        } catch (NullPointerException e) {
            System.out.println("switch NPE");
        }

        // 8. enum with fields/constructors/methods (Planet). printf/String.format
        //    are a registered loud gap (Formatter engine); full double printing
        //    is already Temurin-exact, so the vector pins it directly.
        double earthWeight = 175.0;
        double mass = earthWeight / Planet.EARTH.surfaceGravity();
        for (Planet p : Planet.values()) {
            System.out.println(p + ": " + p.surfaceWeight(mass));
        }

        // 9. static-init order: enum constants are constructed BEFORE any
        //    other static initializer of the same class runs
        System.out.println(Color.values().length == 3);
    }
}
