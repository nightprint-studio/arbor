package corpus.overloads;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.IntFunction;
import java.util.function.Supplier;

/**
 * Lambdas and method references that select a functional overload, and every legal method-reference
 * form: static, bound, unbound, constructor, array constructor, {@code super::m}, {@code this::m}.
 */
public class FunctionalOverloads {

    private final AtomicInteger ticks = new AtomicInteger();

    /** The {@code ExecutorService.submit} shape. */
    static String submit(Runnable task) {
        task.run();
        return "Runnable";
    }

    static <V> String submit(Callable<V> task) {
        return "Callable";
    }

    static String route(Function<String, Integer> function) {
        return "Function:" + function.apply("abc");
    }

    static String route(Consumer<String> consumer) {
        consumer.accept("abc");
        return "Consumer";
    }

    void tick() {
        ticks.incrementAndGet();
    }

    int ticked() {
        return ticks.get();
    }

    public List<String> selectFunctionalOverloads() {
        final List<String> log = new ArrayList<String>();
        List<String> out = new ArrayList<String>();
        out.add(submit(() -> System.out.println("side effect")));  // Runnable: void only
        out.add(submit(() -> 42));                                  // Callable: value only
        out.add(submit(() -> log.add("both")));                     // Callable: more specific than void
        out.add(submit(this::tick));                                // Runnable: void method
        out.add(route((String s) -> s.length()));                   // Function: explicit lambda, non-void
        out.add(route((String s) -> System.out.println(s)));        // Consumer: void body
        out.add(route(s -> {
            log.add(s);
        }));                                                        // Consumer: block without value
        out.add(route(String::length));                             // Function: exact method ref
        out.add(String.valueOf(ticked()));
        return out;
    }

    /** Every method-reference form, each in an assignment context. */
    public static String methodReferenceForms() {
        Function<String, Integer> parse = Integer::parseInt;                // static
        Function<String, String> greet = "hello "::concat;                   // bound
        Function<String, String> upper = String::toUpperCase;               // unbound
        BiFunction<String, Integer, Character> charAt = String::charAt;     // unbound, two args
        Supplier<List<String>> listFactory = ArrayList::new;                 // constructor, inferred
        Function<Integer, StringBuilder> sized = StringBuilder::new;         // constructor overload by arity
        IntFunction<int[]> arrays = int[]::new;                              // array constructor
        IntFunction<String[][]> grids = String[][]::new;

        List<String> names = listFactory.get();
        names.add(greet.apply("bennu"));
        names.add(upper.apply("javac"));
        int[] numbers = arrays.apply(parse.apply("3"));
        String[][] grid = grids.apply(2);
        StringBuilder builder = sized.apply(16);
        builder.append(charAt.apply("xyz", 1)).append(numbers.length).append(grid.length);
        String[] asArray = names.toArray(new String[0]);
        return builder + Arrays.toString(asArray);
    }

    static class Base {
        String describe() {
            return "base";
        }
    }

    static class Derived extends Base {
        @Override
        String describe() {
            return "derived";
        }

        Supplier<String> parent() {
            return super::describe;
        }

        Supplier<String> self() {
            return this::describe;
        }

        String both() {
            return parent().get() + "/" + self().get();
        }
    }

    public static String superAndThis() {
        return new Derived().both();
    }
}
