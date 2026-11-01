public class Cycles {
    // Cyclic graphs are ordinary objects: reachability — not collection —
    // is the only thing Java can observe, so the JVM and J2R must print the
    // same progress values no matter when (or whether) each cycle dies.
    static class Node {
        Node next;
        int v;
        Node(int v) { this.v = v; }
    }
    static int sink;
    public static void main(String[] args) {
        for (int round = 0; round < 50; round++) {
            Node head = new Node(round);
            Node cur = head;
            for (int i = 0; i < 20; i++) {
                cur.next = new Node(round * 100 + i);
                cur = cur.next;
            }
            cur.next = head; // close the ring: a 21-node cycle, dropped whole
            sink += head.v;
            Node a = new Node(-1);
            Node b = new Node(-2);
            a.next = b;
            b.next = a; // two-node mutual cycle, dropped whole
            sink += a.next.v;
        }
        System.out.println("sink " + sink);
        System.out.println("done");
    }
}
