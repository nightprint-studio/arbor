package corpus.sweep;

import java.util.AbstractMap;
import java.util.Collection;
import java.util.Set;

/** Names that look like members of a superclass but are the enclosing class's fields. */
public final class Shadowing<V> extends AbstractMap<String, V> {
    private final V[][] values;

    @SuppressWarnings("unchecked")
    public Shadowing(int rows) {
        this.values = (V[][]) new Object[rows][rows];
    }

    @Override
    public Set<Entry<String, V>> entrySet() {
        return null;
    }

    /** An inner class whose superclass has a {@code values()} METHOD: {@code values[..]} is still the outer field. */
    private final class Row extends AbstractMap<String, V> {
        private final int rowIndex;

        Row(int rowIndex) {
            this.rowIndex = rowIndex;
        }

        V at(int keyIndex) {
            return values[rowIndex][keyIndex];
        }

        public Set<Entry<String, V>> entrySet() {
            return null;
        }

        Collection<V> all() {
            return values();
        }
    }

    /** A constant body's {@code super} is the enum, not the class the enum is nested in. */
    enum Factory {
        PLAIN {
            @Override
            <K> K copy(K key) {
                K fresh = super.copy(key);
                return fresh;
            }
        };

        <K> K copy(K key) {
            return key;
        }
    }
}
