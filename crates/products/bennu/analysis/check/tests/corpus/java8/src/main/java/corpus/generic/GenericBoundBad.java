package corpus.generic;

import java.util.concurrent.atomic.AtomicInteger;

/** Type arguments outside their type parameter's bounds. Twin: {@link GenericBoundOk}. */
public class GenericBoundBad {
    static class NumberBox<T extends Number> {
    }

    static class ComparableBox<T extends Comparable<T>> {
    }

    static class NumberAndComparable<T extends Number & Comparable<T>> {
    }

    static class ExtendsOutsideTheBound extends NumberBox<String> { // error: compiler.err.not.within.bounds
    }

    void stringForANumberBound() {
        NumberBox<String> box = null; // error: compiler.err.not.within.bounds
    }

    void objectForANumberBound() {
        NumberBox<Object> box = null; // error: compiler.err.not.within.bounds
    }

    void creationOutsideTheBound() {
        Object box = new NumberBox<String>(); // error: compiler.err.not.within.bounds
    }

    void typeWithoutTheComparableBound() {
        ComparableBox<Object> box = null; // error: compiler.err.not.within.bounds
    }

    void typeMeetingOnlyOneOfTwoBounds() {
        NumberAndComparable<AtomicInteger> box = null; // error: compiler.err.not.within.bounds
    }

    void wildcardOutsideTheBound() {
        NumberBox<? extends String> box = null; // error: compiler.err.not.within.bounds
    }
}
