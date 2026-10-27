package corpus.sweep.zlast;

import java.util.Set;

public interface Counts<E> {
    Set<Entry<E>> entrySet();

    interface Entry<E> {
        E getElement();

        int getCount();
    }
}
