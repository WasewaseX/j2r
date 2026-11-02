package runner;

import shape.Circle;
import shape.Shape;
import shape.Square;

public class Main {
    private static double total(Shape[] shapes) {
        double t = 0;
        for (int i = 0; i < shapes.length; i++) {
            t += shapes[i].area();
        }
        return t;
    }

    public static void main(String[] args) {
        Shape[] shapes = new Shape[2];
        shapes[0] = new Circle(2.0);
        shapes[1] = new Square(3.0);
        for (int i = 0; i < shapes.length; i++) {
            System.out.println(shapes[i].describe());
        }
        System.out.println("total=" + total(shapes));
    }
}
