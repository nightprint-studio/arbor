package corpus.sweep.table;

import java.util.Map.Entry;

final class FilteredNav<K, V> extends AbstractNav<K, V> {
    @Override
    public Entry<K, V> firstOrNull() {
        return pair(null, null);
    }

    Object first() {
        return entrySet().iterator().next().getKey();
    }
}
