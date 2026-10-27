package corpus.gaps;

/**
 * KNOWN GAP (Guava {@code Multimaps.java:1533}, reported as {@code instantiate-abstract}).
 *
 * <p>Inside {@code Sub}, the simple name {@code Entries} means the member INHERITED from {@code Base} —
 * a class's own and inherited members are in a more inner scope than the members of the class it is
 * nested in (JLS 6.4.1). Bennu picks the enclosing {@code Outer.Entries}, which is abstract.
 */
abstract class Base<K, V> {
    class Entries {
        int size() {
            return 0;
        }
    }
}

final class Outer {
    abstract static class Entries<K, V> {
        abstract int count();
    }

    static class Sub<K, V> extends Base<K, V> {
        Object make() {
            return new Entries();
        }
    }
}
