package corpus.generics;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

/** Bounded type variables, intersection bounds, generic {@code throws}, wildcard capture helpers. */
public final class Bounds {

    private Bounds() {
    }

    public static <T extends Comparable<? super T>> T max(Collection<? extends T> items) {
        Iterator<? extends T> iterator = items.iterator();
        T best = iterator.next();
        while (iterator.hasNext()) {
            T next = iterator.next();
            if (next.compareTo(best) > 0) {
                best = next;
            }
        }
        return best;
    }

    public static <T extends Comparable<? super T>> void insertSorted(List<T> list, T value) {
        int index = Collections.binarySearch(list, value);
        list.add(index < 0 ? -index - 1 : index, value);
    }

    public static <T extends Number & Comparable<T>> T clampTop(T value, T limit) {
        return value.compareTo(limit) > 0 ? limit : value;
    }

    public static <T extends Number & Comparable<T>> double sumAtLeast(Collection<T> values, T floor) {
        double total = 0;
        for (T value : values) {
            if (value.compareTo(floor) >= 0) {
                total += value.doubleValue();
            }
        }
        return total;
    }

    public interface ThrowingSupplier<T, X extends Exception> {
        T get() throws X;
    }

    /** {@code X} appears only in {@code throws}: inferred as RuntimeException when the body throws nothing checked. */
    public static <T, X extends Exception> T attempt(ThrowingSupplier<T, X> supplier) throws X {
        return supplier.get();
    }

    public static <T, X extends Throwable> T require(T value, Supplier<? extends X> failure) throws X {
        if (value == null) {
            throw failure.get();
        }
        return value;
    }

    /** The capture helper idiom: a {@code List<?>} cannot be written to, a {@code List<E>} can. */
    public static void swapEnds(List<?> list) {
        swapEndsCaptured(list);
    }

    private static <E> void swapEndsCaptured(List<E> list) {
        if (list.size() < 2) {
            return;
        }
        E first = list.get(0);
        list.set(0, list.get(list.size() - 1));
        list.set(list.size() - 1, first);
    }

    public static double total(Collection<? extends Number> numbers) {
        double sum = 0;
        for (Number number : numbers) {
            sum += number.doubleValue();
        }
        return sum;
    }

    public static void fillSquares(List<? super Integer> sink, int count) {
        for (int i = 0; i < count; i++) {
            sink.add(i * i);
        }
    }

    public static <K, V extends Comparable<? super V>> List<Map.Entry<K, V>> sortedByValue(Map<K, V> map) {
        List<Map.Entry<K, V>> entries = new ArrayList<Map.Entry<K, V>>(map.entrySet());
        Collections.sort(entries, new Comparator<Map.Entry<K, V>>() {
            @Override
            public int compare(Map.Entry<K, V> left, Map.Entry<K, V> right) {
                return left.getValue().compareTo(right.getValue());
            }
        });
        return entries;
    }

    public static <K, V extends Comparable<? super V>> Map.Entry<K, V> smallestEntry(Map<K, V> map) {
        List<Map.Entry<K, V>> entries = new ArrayList<Map.Entry<K, V>>(map.entrySet());
        entries.sort(Map.Entry.<K, V>comparingByValue());
        return entries.get(0);
    }

    private static String read(String name) throws IOException {
        if (name.isEmpty()) {
            throw new FileNotFoundException("no name");
        }
        return name;
    }

    public static String demo(Map<String, Integer> scores) throws IOException {
        List<Number> sink = new ArrayList<Number>();
        fillSquares(sink, 4);
        List<Integer> sorted = new ArrayList<Integer>();
        insertSorted(sorted, 5);
        insertSorted(sorted, 2);
        swapEnds(sorted);
        int clamped = clampTop(7, 5);
        double atLeast = sumAtLeast(Arrays.asList(1.5, 2.5, 3.5), 2.0);
        String unchecked = attempt(() -> "config");
        String loaded = attempt(() -> read("settings"));
        Integer required = require(Integer.valueOf(3), () -> new IllegalStateException("missing"));
        return max(sorted) + unchecked + loaded + clamped + atLeast + total(sink) + required
                + sortedByValue(scores) + smallestEntry(scores).getKey();
    }
}
