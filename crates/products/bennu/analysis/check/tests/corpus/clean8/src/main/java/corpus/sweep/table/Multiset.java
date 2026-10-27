package corpus.sweep.table;

import java.util.Set;

public interface Multiset<E> {
    Set<Entry<E>> entrySet();

    interface Entry<E> {
        E getElement();

        int getCount();
    }
}
