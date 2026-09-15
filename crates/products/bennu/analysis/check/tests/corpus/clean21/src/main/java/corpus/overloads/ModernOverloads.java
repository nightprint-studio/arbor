package corpus.overloads;

import java.util.List;
import java.util.concurrent.Callable;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.IntFunction;
import java.util.function.Supplier;
import java.util.stream.Stream;

/**
 * Overloads selected by static type, by lambda shape and by arity of {@code var} lambda parameters;
 * method references to record constructors and {@code Interface.super::method}.
 */
public final class ModernOverloads {

    private ModernOverloads() {
    }

    public record Point(int x, int y) {
        public Point(int both) {
            this(both, both);
        }

        public static Point origin() {
            return new Point(0, 0);
        }
    }

    static String accept(Object value) {
        return "Object";
    }

    static String accept(CharSequence value) {
        return "CharSequence";
    }

    static String accept(Point point) {
        return "Point";
    }

    static String run(Runnable task) {
        task.run();
        return "Runnable";
    }

    static <V> String run(Callable<V> task) {
        return "Callable";
    }

    static String combine(BiFunction<String, String, String> joiner) {
        return joiner.apply("a", "b");
    }

    static String combine(Function<String, String> single) {
        return single.apply("a");
    }

    public static List<String> resolve() {
        var text = "hello";
        var builder = new StringBuilder(text);
        Object boxed = 5;
        var counter = new int[1];
        return List.of(
                accept(text),                   // CharSequence
                accept(builder),                // CharSequence
                accept(boxed),                  // Object: the static type decides
                accept(Point.origin()),         // Point
                run(() -> counter[0]++),        // Callable: an increment is also a value
                run(() -> {
                    counter[0]++;
                }),                             // Runnable: a block without a value
                run(() -> text.length()),       // Callable
                combine((var left, var right) -> left + right),
                combine((var only) -> only.repeat(2)));
    }

    public static String constructors() {
        BiFunction<Integer, Integer, Point> twoArgs = Point::new;
        Function<Integer, Point> oneArg = Point::new;
        Supplier<Point> origin = Point::origin;
        IntFunction<Point[]> arrays = Point[]::new;
        var points = Stream.of(1, 2, 3).map(Point::new).toArray(Point[]::new);
        var grid = arrays.apply(points.length);
        grid[0] = twoArgs.apply(1, 2);
        return grid[0] + " " + oneArg.apply(3) + " " + origin.get() + " " + points.length;
    }

    interface Greeter {
        default String greet() {
            return "hello";
        }
    }

    static final class LoudGreeter implements Greeter {
        @Override
        public String greet() {
            Supplier<String> quiet = Greeter.super::greet;
            return quiet.get().toUpperCase();
        }
    }

    public static String demo() {
        return resolve() + constructors() + new LoudGreeter().greet();
    }
}
