package corpus.generic;

import java.util.List;

/** Legal twins of {@link GenericWildcardBad}: what a wildcard does accept. */
public class GenericWildcardOk {
    <T> void duplicateTheFirst(List<T> values) {
        values.add(values.get(0));
    }

    void addNullToAnExtendsWildcard(List<? extends Number> numbers) {
        numbers.add(null);
    }

    void readFromAnExtendsWildcard(List<? extends Number> numbers) {
        Number first = numbers.get(0);
    }

    void addToASuperWildcard(List<? super Integer> sink) {
        sink.add(1);
    }

    void readObjectsFromAnUnboundedWildcard(List<?> any) {
        Object first = any.get(0);
        int size = any.size();
    }

    void removeThroughAnExtendsWildcard(List<? extends Number> numbers) {
        numbers.remove(0);
        numbers.clear();
    }

    void captureThroughAGenericHelper(List<?> any) {
        duplicateTheFirst(any);
    }

    void getOnAnExtendsWildcardBox(GenericApi.Box<? extends Number> box) {
        Number value = box.get();
    }
}
