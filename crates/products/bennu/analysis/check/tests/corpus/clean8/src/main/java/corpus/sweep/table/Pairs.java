package corpus.sweep.table;

import java.util.AbstractMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Map.Entry;

/** {@code Entry} here is {@code Map.Entry} (imported); {@code Multiset.Entry} is another type of the same name next door. */
public abstract class Pairs<K, V> implements Iterable<Entry<K, V>> {
    abstract Map<K, V> backing();

    abstract Entries<K, V> entries();

    public Iterator<Entry<K, V>> iterator() {
        final Iterator<Entry<K, V>> inner = backing().entrySet().iterator();
        return new Iterator<Entry<K, V>>() {
            public boolean hasNext() {
                return inner.hasNext();
            }

            public Entry<K, V> next() {
                return nextEntry(inner.next());
            }
        };
    }

    Entry<K, V> nextEntry(Entry<K, V> raw) {
        return new Through(raw.getKey(), raw.getValue());
    }

    final class Through extends AbstractMap.SimpleEntry<K, V> implements Entry<K, V> {
        Through(K k, V v) {
            super(k, v);
        }
    }

    abstract static class Entries<K, V> {
        abstract int size();
    }

    static class Plain<K, V> extends Pairs<K, V> {
        Map<K, V> backing() {
            return null;
        }

        Entries<K, V> entries() {
            return new Entries<K, V>() {
                int size() {
                    return 0;
                }
            };
        }
    }
}
