public class GcIdentity {
    // One object aliased through an array and a static field: mutation via
    // one path must be visible through the other, at every point in time —
    // regardless of whether a collection happened in between.
    static class Box {
        int x;
    }
    static Box[] arr = new Box[3];
    static Box keep;
    static int sink;
    public static void main(String[] args) {
        Box b = new Box();
        b.x = 41;
        arr[1] = b;
        keep = b;
        System.out.println(arr[1] == b);
        System.out.println(keep == b);
        arr[1].x++;
        System.out.println(keep.x);
        for (int i = 0; i < 10000; i++) {
            Box junk = new Box();
            junk.x = i;
            sink += junk.x & 1;
        }
        arr[2] = keep;
        arr[2].x += 10;
        System.out.println(keep.x);
        System.out.println(b.x);
        System.out.println("sink-nonneg " + (sink >= 0));
    }
}
