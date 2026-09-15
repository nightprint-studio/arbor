package corpus.lambda;

import java.util.concurrent.Callable;
import java.util.function.BiConsumer;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.function.ToIntFunction;

/** Clean functional-interface targets shared by the lambda cases. Must stay error-free. */
public final class LambdaTargets {
    private LambdaTargets() {
    }

    public static void run(Runnable task) {
    }

    public static <T> T call(Callable<T> task) {
        return null;
    }

    public static void submit(Runnable task) {
    }

    public static <T> void submit(Callable<T> task) {
    }

    public static void mapString(Function<String, Integer> mapper) {
    }

    public static void overloadedMapper(Function<String, Integer> mapper) {
    }

    public static void overloadedMapper(ToIntFunction<String> mapper) {
    }

    public static void consumeTwo(BiConsumer<String, Integer> consumer) {
    }

    public static void supply(Supplier<String> supplier) {
    }

    public static void predicate(Predicate<String> test) {
    }
}
