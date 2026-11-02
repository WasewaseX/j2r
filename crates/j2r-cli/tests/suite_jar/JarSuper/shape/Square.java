package shape;

public class Square extends Shape {
    private final double side;

    public Square(double side) {
        super("square");
        this.side = side;
    }

    public double area() {
        return side * side;
    }
}
