package corpus.generic;

import java.util.ArrayList;
import java.util.List;

/** Legal twins of {@link GenericReifyBad}: the reifiable spellings of the same intents. */
public class GenericReifyOk<T> {
    private final Class<T> type;

    GenericReifyOk(Class<T> type) {
        this.type = type;
    }

    T castThroughTheClassObject(Object value) {
        return type.cast(value);
    }

    @SuppressWarnings("unchecked")
    T[] arrayThroughAnUncheckedCast(int size) {
        return (T[]) new Object[size];
    }

    Object arrayOfAWildcardType() {
        return new List<?>[3];
    }

    Object arrayOfARawType() {
        return new List[3];
    }

    boolean instanceofARawType(Object value) {
        return value instanceof List;
    }

    boolean instanceofAWildcardType(Object value) {
        return value instanceof List<?>;
    }

    void boxedTypeArguments() {
        List<Integer> values = new ArrayList<Integer>();
    }

    Class<?> classLiteralOfARawType() {
        return List.class;
    }
}
