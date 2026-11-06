// Singleton-factory differential fixture (review P1): the three canonical
// Java singleton shapes, all initialized through <clinit>, pinned against
// the JVM — factory identity, constructor-exactly-once, the static-init
// block form, and the initialization-on-demand-holder idiom with a
// laziness proof (the holder class is untouched until first getInstance()).
public class SingletonFactory {
    public static void main(String[] args) {
        // 1. Factory singleton: INSTANCE built in <clinit> through a static
        //    factory method calling the private constructor.
        FactorySingleton a = FactorySingleton.getInstance();
        FactorySingleton b = FactorySingleton.getInstance();
        System.out.println(a == b);
        System.out.println(a.seed());
        a.tick();
        System.out.println(b.seed());
        System.out.println(FactorySingleton.created);

        // 2. Static-init-block singleton: assignment inside `static { }`.
        BlockSingleton c = BlockSingleton.getInstance();
        System.out.println(c.tag());
        System.out.println(BlockSingleton.instance != null);
        System.out.println(BlockSingleton.created);

        // 3. Holder idiom: laziness is observable — reading the outer
        //    class's counter must NOT create the instance.
        System.out.println(HolderSingleton.created);
        HolderSingleton h = HolderSingleton.getInstance();
        System.out.println(h.id());
        System.out.println(HolderSingleton.created);
        System.out.println(HolderSingleton.getInstance() == h);
    }
}

class FactorySingleton {
    static int created = 0;
    private static final FactorySingleton INSTANCE = create();

    private int seed;

    private FactorySingleton(int seed) {
        this.seed = seed;
        created++;
    }

    private static FactorySingleton create() {
        return new FactorySingleton(42);
    }

    static FactorySingleton getInstance() {
        return INSTANCE;
    }

    int seed() {
        return seed;
    }

    void tick() {
        seed++;
    }
}

class BlockSingleton {
    static int created = 0;
    static BlockSingleton instance;

    static {
        instance = new BlockSingleton();
    }

    private BlockSingleton() {
        created++;
    }

    static BlockSingleton getInstance() {
        return instance;
    }

    String tag() {
        return "block";
    }
}

class HolderSingleton {
    static int created = 0;

    private static class HolderBox {
        static final HolderSingleton INSTANCE = new HolderSingleton();
    }

    private final int id = 7;

    private HolderSingleton() {
        created++;
    }

    static HolderSingleton getInstance() {
        return HolderBox.INSTANCE;
    }

    int id() {
        return id;
    }
}
