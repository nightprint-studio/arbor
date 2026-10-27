package corpus.infer;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.function.Function;

/** The project's own types the inference cases read members of. */
public final class InferTypes {
    private InferTypes() { }

    public static final class Box<T> {
        private final T value;

        public Box(T value) { this.value = value; }

        public T get() { return value; }

        public <R> Box<R> map(Function<? super T, ? extends R> fn) { return new Box<>(fn.apply(value)); }
    }

    public static final class Pair<A, B> {
        private final A first;
        private final B second;

        private Pair(A first, B second) { this.first = first; this.second = second; }

        public static <A, B> Pair<A, B> of(A first, B second) { return new Pair<>(first, second); }

        public A first() { return first; }

        public B second() { return second; }

        public Pair<B, A> swap() { return new Pair<>(second, first); }
    }

    @SuppressWarnings("serial")
    public static class Names extends ArrayList<String> { }

    public static final class Util {
        private Util() { }

        public static <T extends Comparable<T>> T max(List<T> items) { return items.get(0); }

        public static <T> List<T> copyOf(List<T> items) { return new ArrayList<>(items); }

        public static <T> Comparator<T> reversed(Comparator<T> original) { return original.reversed(); }
    }

    public enum Color { RED, GREEN }

    public record Point(int x, int y) { }

    public static abstract class Animal {
        public abstract String speak();

        public String kind() { return "animal"; }
    }

    public static final class Dog extends Animal {
        @Override
        public String speak() { return "woof"; }
    }

    public interface Shape {
        default String describe() { return "shape"; }
    }

    public static final class Circle implements Shape { }
}
