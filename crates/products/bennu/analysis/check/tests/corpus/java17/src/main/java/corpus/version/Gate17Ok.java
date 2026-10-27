package corpus.version;

import java.util.Collection;
import java.util.List;
import java.util.stream.Stream;

/** Everything Java 17 allows: records, sealed types, `instanceof` patterns, and what 14-16 added. */
public class Gate17Ok {
    sealed interface Shape permits Circle, Square, Blob {
    }

    record Circle(double radius) implements Shape {
        Circle {
            if (radius < 0) {
                throw new IllegalArgumentException();
            }
        }
    }

    static final class Square implements Shape {
    }

    static non-sealed class Blob implements Shape {
    }

    static class Counter {
        class Inner {
            static int created; // a static member of an inner class needs 16
        }
    }

    double area(Shape shape) {
        if (shape instanceof Circle circle && circle.radius() > 0) {
            return Math.PI * circle.radius() * circle.radius();
        }
        return 0;
    }

    int length(Object value) {
        if (!(value instanceof String text)) {
            return -1;
        }
        return text.length();
    }

    boolean parameterizedInstanceof(Collection<String> names) {
        return names instanceof List<String> list && list.isEmpty(); // reifiable when the cast is checked: 16
    }

    String kind(Shape shape) {
        return switch (shape.getClass().getSimpleName()) {
            case "Circle" -> "round";
            case "Square" -> "boxy";
            default -> "other";
        };
    }

    void localRecordInterfaceAndEnum() {
        record Pair(int left, int right) {
        }
        interface Visitor {
            void visit(Pair pair);
        }
        enum Mode { ON, OFF }
        Visitor visitor = pair -> System.out.println(pair.left() + pair.right() + Mode.ON.name());
        visitor.visit(new Pair(1, 2));
    }

    List<String> streamToList(Stream<String> names) {
        return names.toList(); // Java 16 API
    }

    String textBlock() {
        return """
            hello
            """;
    }
}
