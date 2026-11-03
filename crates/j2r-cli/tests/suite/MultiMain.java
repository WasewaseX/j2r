// Audit A3 fixture: two main-bearing classes in one source file. The JVM
// runs `java MultiMain` — the class named like the file — and j2r must pick
// the same one (source-stem preference), not whichever class read_dir
// happens to list first. Before the fix this was directory-order luck.
public class MultiMain {
    public static void main(String[] args) {
        System.out.println("picked MultiMain");
        Other.other();
    }
}

class Other {
    public static void other() {
        System.out.println("Other helper ran");
    }

    public static void main(String[] args) {
        System.out.println("picked Other");
    }
}
