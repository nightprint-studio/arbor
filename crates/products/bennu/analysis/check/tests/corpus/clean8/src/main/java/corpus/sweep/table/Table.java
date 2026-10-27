package corpus.sweep.table;

import java.util.Set;

/** The outer type of the nested {@code Cell} that its implementations inherit (Guava's {@code Table.Cell}). */
public interface Table<R, V> {
    Set<Cell<R, V>> cellSet();

    interface Cell<R, V> {
        R getRow();

        V getValue();
    }
}
