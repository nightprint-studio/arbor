package corpus.generic;

import java.util.List;
import java.util.Map;

/** Writes through wildcards that cannot accept the value. Twin: {@link GenericWildcardOk}. */
public class GenericWildcardBad {
    void addToAnExtendsWildcard(List<? extends Number> numbers) {
        numbers.add(1); // error: compiler.err.cant.apply.symbols
    }

    void addToAnUnboundedWildcard(List<?> any) {
        any.add("x"); // error: compiler.err.cant.apply.symbols
    }

    void putIntoAWildcardMap(Map<?, ?> values) {
        values.put("key", "value"); // error: compiler.err.cant.apply.symbol
    }

    void addADoubleToASuperIntegerWildcard(List<? super Integer> sink) {
        sink.add(1.5); // error: compiler.err.cant.apply.symbols
    }

    void setOnAnExtendsWildcardBox(GenericApi.Box<? extends Number> box) {
        box.set(1); // error: compiler.err.cant.apply.symbol
    }
}
