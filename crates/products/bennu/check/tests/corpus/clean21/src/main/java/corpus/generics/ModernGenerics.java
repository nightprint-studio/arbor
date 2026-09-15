package corpus.generics;

import java.util.Collection;
import java.util.List;
import java.util.function.Function;
import java.util.function.Supplier;

/** Generic records, a generic sealed interface matched exhaustively, bounds and inference with {@code var}. */
public final class ModernGenerics {

    private ModernGenerics() {
    }

    public record Page<T>(List<T> items, int number, int size) {
        public Page {
            items = List.copyOf(items);
            if (number < 0 || size <= 0) {
                throw new IllegalArgumentException("page " + number + "/" + size);
            }
        }

        public static <T> Page<T> of(List<T> all, int number, int size) {
            var from = Math.min(all.size(), number * size);
            var to = Math.min(all.size(), from + size);
            return new Page<>(all.subList(from, to), number, size);
        }

        public <R> Page<R> map(Function<? super T, ? extends R> mapper) {
            return new Page<>(items.stream().<R>map(mapper).toList(), number, size);
        }
    }

    public sealed interface Result<T> {
        record Ok<T>(T value) implements Result<T> {
        }

        record Err<T>(String message) implements Result<T> {
        }

        default <R> Result<R> map(Function<? super T, ? extends R> mapper) {
            return switch (this) {
                case Ok<T> ok -> new Ok<>(mapper.apply(ok.value()));
                case Err<T> err -> new Err<>(err.message());
            };
        }

        default T orElse(T fallback) {
            return this instanceof Ok<T>(var value) ? value : fallback;
        }

        static <T> Result<T> attempt(Supplier<? extends T> supplier) {
            try {
                return new Ok<>(supplier.get());
            } catch (RuntimeException e) {
                return new Err<>(String.valueOf(e.getMessage()));
            }
        }
    }

    public static <T extends Comparable<? super T>> T maxOf(Collection<? extends T> values) {
        T best = null;
        for (T value : values) {
            if (best == null || value.compareTo(best) > 0) {
                best = value;
            }
        }
        return best;
    }

    public static <T extends Number & Comparable<T>> List<T> clampAll(List<T> values, T ceiling) {
        return values.stream().map(value -> value.compareTo(ceiling) > 0 ? ceiling : value).toList();
    }

    public static String demo() {
        var page = Page.of(List.of("a", "bb", "ccc", "dddd"), 1, 2).map(String::length);
        Result<Integer> parsed = Result.attempt(() -> Integer.parseInt("42"));
        var doubled = parsed.map(value -> value * 2).orElse(-1);
        var failed = Result.<Integer>attempt(() -> Integer.parseInt("x")).orElse(0);
        var biggest = maxOf(List.of(3, 9, 4));
        var clamped = clampAll(List.of(1.5, 7.5), 5.0);
        return page + " " + doubled + " " + failed + " " + biggest + " " + clamped;
    }
}
