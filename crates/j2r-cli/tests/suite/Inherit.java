abstract class Shape {
    abstract double area();
    abstract String name();
    public String describe() {
        return name() + " with area " + area();
    }
}

class Circle extends Shape {
    double r;
    Circle(double r) { this.r = r; }
    double area() { return 3.0 * r * r; }
    String name() { return "circle"; }
}

class Square extends Shape {
    double side;
    Square(double side) { this.side = side; }
    double area() { return side * side; }
    String name() { return "square"; }
    @Override
    public String describe() {
        return "SPECIAL " + super.describe();
    }
}

class Rect extends Square {
    double h;
    Rect(double w, double h) { super(w); this.h = h; }
    double area() { return side * h; }
    String name() { return "rect"; }
}

public class Inherit {
    public static void main(String[] args) {
        Shape[] shapes = new Shape[3];
        shapes[0] = new Circle(2.0);
        shapes[1] = new Square(3.0);
        shapes[2] = new Rect(2.0, 5.0);
        double total = 0;
        for (Shape s : shapes) {
            System.out.println(s.describe());
            total += s.area();
        }
        System.out.println(total);
        Shape s = shapes[2];
        System.out.println(s instanceof Rect);
        System.out.println(s instanceof Square);
        System.out.println(s instanceof Shape);
        System.out.println(s instanceof Circle);
        Rect r = (Rect) s;
        System.out.println(r.side + r.h);
        try {
            Circle c = (Circle) s;
            System.out.println(c);
        } catch (ClassCastException e) {
            System.out.println("cast failed");
        }
        Object o = shapes[1];
        System.out.println(o instanceof Shape);
        System.out.println(shapes.length);
    }
}
