package shape;

public class Circle extends Shape {
    private final double r;

    public Circle(double r) {
        super("circle");
        this.r = r;
    }

    public double area() {
        return 3.0 * r * r;
    }
}
