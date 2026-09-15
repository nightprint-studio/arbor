package corpus.generic;

import java.util.List;
import java.util.Map;
import java.util.function.Function;

/** Clean generic call targets shared by the generic cases. Must stay error-free. */
public final class GenericApi {
    private GenericApi() {
    }

    public static class Box<T> {
        private T value;

        public void set(T value) {
            this.value = value;
        }

        public T get() {
            return value;
        }

        public <R> Box<R> map(Function<? super T, ? extends R> mapper) {
            Box<R> mapped = new Box<R>();
            mapped.set(mapper.apply(value));
            return mapped;
        }
    }

    public static class StringBox extends Box<String> {
    }

    public interface Holder<T> {
        T raw();

        default T orDefault(T fallback) {
            T current = raw();
            return current != null ? current : fallback;
        }
    }

    public static <T extends Number> double num(T value) {
        return value.doubleValue();
    }

    public static double sum(List<? extends Number> values) {
        return 0;
    }

    public static void fill(List<? super Integer> values) {
    }

    public static double sumExact(List<Number> values) {
        return 0;
    }

    public static <T extends Comparable<T>> T maxOf(T first, T second) {
        return first.compareTo(second) >= 0 ? first : second;
    }

    public static <T> T first(List<T> values) {
        return values.get(0);
    }

    public static <K, V> void put(Map<K, V> map, K key, V value) {
        map.put(key, value);
    }
}
