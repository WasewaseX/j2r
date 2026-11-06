// Cyclic class-initialization differential fixture (review P1): two pairs
// of classes whose <clinit> bodies reference each other's statics, pinning
// JLS 12.4.2 in-progress recursion — a recursive initialization request
// from the SAME thread returns immediately, so the second class observes
// either the already-assigned value (cycle 1) or the default value
// (cycle 2), depending on assignment order. HotSpot and j2r must agree
// byte-for-byte on all six numbers.
public class CyclicInit {
    public static void main(String[] args) {
        // Cycle 1: InitA assigns its static BEFORE recursing into InitB;
        // InitB's clinit reads InitA.s (assigned: 10); back in InitA,
        // after reads InitB.t (assigned: 110).
        System.out.println(InitA.s);
        System.out.println(InitB.t);
        System.out.println(InitA.after);

        // Cycle 2: InitC recurses into InitD BEFORE assigning its own
        // static; InitD's clinit reads InitC.x while InitC is in progress
        // — the DEFAULT value (0) — so y becomes 1, and x becomes 1.
        System.out.println(InitC.x);
        System.out.println(InitD.y);
        System.out.println(InitC.own);
    }
}

class InitA {
    static int s = 10;
    static int after = InitB.t;
}

class InitB {
    static int t = InitA.s + 100;
}

class InitC {
    static int x = InitD.y;
    static int own = 5;
}

class InitD {
    static int y = InitC.x + 1;
}
