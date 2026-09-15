package corpus.structure;

import java.util.ArrayList;
import java.util.List;

/**
 * Interface default and static methods, {@code Object} methods redeclared on an interface, an
 * abstract class with a protected constructor, constructor chaining, {@code super.m()} reaching an
 * inherited default method, and {@code X.super.m()} resolving a default-method diamond.
 */
public final class Shapes {

    private Shapes() {
    }

    public interface Shape {
        double area();

        default String describe() {
            return name() + " of area " + format(area());
        }

        default String name() {
            return getClass().getSimpleName();
        }

        static String format(double value) {
            return String.format("%.2f", value);
        }

        /** An {@code Object} method redeclared: {@code Shape} stays a functional interface. */
        @Override
        String toString();
    }

    public abstract static class AbstractShape implements Shape {
        private final String label;

        protected AbstractShape(String label) {
            this.label = label;
        }

        @Override
        public String name() {
            return label;
        }

        @Override
        public boolean equals(Object other) {
            if (!(other instanceof AbstractShape)) {
                return false;
            }
            AbstractShape shape = (AbstractShape) other;
            return label.equals(shape.label) && Double.compare(area(), shape.area()) == 0;
        }

        @Override
        public int hashCode() {
            return label.hashCode();
        }

        @Override
        public String toString() {
            return describe();
        }
    }

    public static final class Circle extends AbstractShape {
        private final double radius;

        public Circle(double radius) {
            super("circle");
            this.radius = radius;
        }

        @Override
        public double area() {
            return Math.PI * radius * radius;
        }
    }

    public static class Rect extends AbstractShape {
        private final double width;
        private final double height;

        public Rect(double width, double height) {
            super("rect");
            this.width = width;
            this.height = height;
        }

        protected Rect(double side) {
            this(side, side);
        }

        @Override
        public double area() {
            return width * height;
        }

        @Override
        public String describe() {
            return "rectangle " + width + "x" + height + ", " + super.describe();
        }
    }

    public static final class Square extends Rect {
        public Square(double side) {
            super(side);
        }
    }

    public interface Printable {
        default String render() {
            return "printable";
        }
    }

    public interface Exportable {
        default String render() {
            return "exportable";
        }
    }

    public static final class Report implements Printable, Exportable {
        @Override
        public String render() {
            return Printable.super.render() + "+" + Exportable.super.render();
        }
    }

    public static String demo() {
        List<Shape> shapes = new ArrayList<Shape>();
        shapes.add(new Circle(1));
        shapes.add(new Rect(2, 3));
        shapes.add(new Square(2));
        shapes.add(() -> 1.0);
        StringBuilder out = new StringBuilder(Shape.format(0.5));
        double total = 0;
        for (Shape shape : shapes) {
            total += shape.area();
            out.append(shape.describe()).append('\n');
        }
        boolean same = new Rect(2, 2).equals(new Square(2));
        return out.append(total).append(same).append(new Report().render()).toString();
    }
}
