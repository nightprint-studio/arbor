package corpus.sweep;

import java.util.AbstractMap;
import java.util.AbstractSet;
import java.util.Iterator;
import java.util.Map;
import java.util.Map.Entry;
import java.util.Set;

/**
 * Guard (Guava {@code Maps.java:3867}, once reported as {@code override-overrides-nothing}; reproduced only partly, kept as a regression guard).
 *
 * <p>The anonymous {@code new EntrySet<K, V>() { … map() … }} extends the {@code EntrySet} nested in
 * THIS class; another type of the same simple name exists elsewhere in the project, and the method
 * {@code map()} is looked up on that one.
 */
final class AnonymousOverCollidingName {
    abstract static class EntrySet<K, V> extends AbstractSet<Entry<K, V>> {
        abstract Map<K, V> map();

        @Override
        public int size() {
            return map().size();
        }
    }

    abstract static class IteratorBased<K, V> extends AbstractMap<K, V> {
        abstract Iterator<Entry<K, V>> entryIterator();

        @Override
        public Set<Entry<K, V>> entrySet() {
            return new EntrySet<K, V>() {
                @Override
                Map<K, V> map() {
                    return IteratorBased.this;
                }

                @Override
                public Iterator<Entry<K, V>> iterator() {
                    return entryIterator();
                }
            };
        }
    }
}

/** The unrelated type of the same name, found first by the project-wide simple-name map. */
abstract class EntrySet<E> extends AbstractSet<E> {
    abstract int version();
}
