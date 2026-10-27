package corpus.sweep;

import java.util.ArrayList;
import java.util.List;

/** Guava's {@code Function} extends the JDK's and redeclares {@code apply} over it: one abstract method, still a functional interface. */
public class FunctionBridge {
    public interface Fn<F, T> extends java.util.function.Function<F, T> {
        @Override
        T apply(F input);

        @Override
        boolean equals(Object other);
    }

    /** Guava's {@code Predicate}: {@code apply} abstract, the JDK's abstract {@code test} turned {@code default}. */
    public interface Pred<T> extends java.util.function.Predicate<T> {
        boolean apply(T input);

        @Override
        default boolean test(T input) {
            return apply(input);
        }
    }

    static <K, V> boolean removeIf(Pred<? super java.util.Map.Entry<K, V>> predicate) {
        return predicate.apply(null);
    }

    boolean positive() {
        return removeIf((java.util.Map.Entry<String, Integer> entry) -> entry.getValue() > 0);
    }

    static <A, B> List<B> transform(List<A> from, Fn<? super A, ? extends B> function) {
        List<B> out = new ArrayList<B>();
        for (A a : from) {
            out.add(function.apply(a));
        }
        return out;
    }

    List<Integer> lengths(List<String> names) {
        return transform(names, s -> s.length());
    }

    List<String> upper(List<String> names) {
        return transform(names, String::toUpperCase);
    }
}
