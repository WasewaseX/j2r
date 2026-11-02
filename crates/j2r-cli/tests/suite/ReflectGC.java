import java.lang.reflect.Method;

public class ReflectGC {
    static class Node {
        Node next;
        int pad;
    }

    public static void main(String[] args) throws Exception {
        Class<?> cls = Node.class;
        Method mark = ReflectGC.class.getDeclaredMethod("mark");
        // Build 200k linked nodes (garbage after the loop drops the head).
        Node head = new Node();
        Node cur = head;
        for (int i = 0; i < 200000; i++) {
            Node n = new Node();
            n.next = cur;
            cur = n;
        }
        // Collect: the linked ring dies, but interned Class objects and the
        // Method object held in a local are roots.
        System.gc();
        // Identity and metadata survive the collection.
        System.out.println(cls == Node.class);
        System.out.println(cls.getName());
        // The method table lookup still dispatches into compiled code.
        System.out.println(mark.invoke(null));
        // New allocations after GC keep working.
        Node fresh = new Node();
        System.out.println(fresh.getClass() == cls);
        System.out.println(fresh.getClass().getSimpleName());
    }

    public static String mark() {
        return "alive-after-gc";
    }
}
