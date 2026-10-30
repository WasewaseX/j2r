public class Objects {
    static int counter = 0;
    private int x;
    private int y;
    private String label;

    Objects(int x, int y) {
        this.x = x;
        this.y = y;
        this.label = "pt" + counter;
        counter++;
    }

    Objects() {
        this(0, 0);
    }

    int getX() { return x; }
    int getY() { return y; }
    String label() { return label; }

    void move(int dx, int dy) {
        x += dx;
        y += dy;
    }

    @Override
    public String toString() {
        return label + "(" + x + "," + y + ")";
    }

    @Override
    public boolean equals(Object other) {
        if (!(other instanceof Objects)) return false;
        Objects o = (Objects) other;
        return o.x == x && o.y == y;
    }

    @Override
    public int hashCode() {
        return x * 31 + y;
    }

    public static void main(String[] args) {
        Objects p = new Objects(3, 4);
        Objects q = new Objects();
        System.out.println(p);
        System.out.println(q);
        p.move(1, -2);
        System.out.println(p);
        System.out.println(p.getX() + p.getY());
        System.out.println(counter);
        System.out.println(p.equals(q));
        System.out.println(p.equals(p));
        Objects p2 = new Objects(4, 2);
        System.out.println(p.equals(p2));
        System.out.println(p.hashCode() == p2.hashCode());
        System.out.println(p instanceof Objects);
        Object o = p;
        System.out.println(o.equals(p2));
    }
}
