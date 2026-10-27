package corpus.sweep.table;

import java.util.AbstractSet;
import java.util.Iterator;
import java.util.Set;

abstract class AbstractTable<R, V> implements Table<R, V> {
    @Override
    public Set<Cell<R, V>> cellSet() {
        return new AbstractSet<Cell<R, V>>() {
            @Override
            public Iterator<Cell<R, V>> iterator() {
                return null;
            }

            @Override
            public int size() {
                return 0;
            }
        };
    }
}
