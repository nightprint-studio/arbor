package corpus.generic;

/** Legal twins of {@link GenericBoundBad}. */
public class GenericBoundOk {
    static class NumberBox<T extends Number> {
    }

    static class ComparableBox<T extends Comparable<T>> {
    }

    static class NumberAndComparable<T extends Number & Comparable<T>> {
    }

    static class ExtendsWithinTheBound extends NumberBox<Long> {
    }

    void withinTheNumberBound() {
        NumberBox<Integer> integers = null;
        NumberBox<Number> numbers = null;
    }

    void creationWithinTheBound() {
        Object box = new NumberBox<Double>();
    }

    void withinTheComparableBound() {
        ComparableBox<String> box = null;
    }

    void withinBothBounds() {
        NumberAndComparable<Integer> box = null;
    }

    void wildcardsWithinTheBound() {
        NumberBox<?> any = null;
        NumberBox<? extends Integer> integers = null;
        NumberBox<? super Integer> supers = null;
    }

    <T extends Number> void typeVariableWithinTheBound() {
        NumberBox<T> box = null;
    }
}
