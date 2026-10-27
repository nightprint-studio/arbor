package corpus.sweep.table;

import java.util.AbstractMap;
import java.util.Set;

/** {@code Entry} and {@code SimpleEntry} are inherited from {@code AbstractMap}: no import, and not the {@code Multiset.Entry} next door. */
abstract class AbstractNav<K, V> extends AbstractMap<K, V> {
    public Entry<K, V> firstOrNull() {
        return null;
    }

    @Override
    public Set<Entry<K, V>> entrySet() {
        return null;
    }

    Entry<K, V> pair(K key, V value) {
        return new SimpleEntry<K, V>(key, value);
    }
}
