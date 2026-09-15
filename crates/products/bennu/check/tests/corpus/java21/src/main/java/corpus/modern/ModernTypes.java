package corpus.modern;

/** Clean records, sealed types and enums shared by the modern cases. Must stay error-free. */
public final class ModernTypes {
    private ModernTypes() {
    }

    public record Point(int x, int y) {
        public Point {
        }

        public static Point origin() {
            return new Point(0, 0);
        }

        public int sum() {
            return x + y;
        }
    }

    public sealed interface Shape permits Circle, Square {
    }

    public record Circle(double radius) implements Shape {
    }

    public record Square(double side) implements Shape {
    }

    public enum Mode {
        FAST, SLOW
    }
}
