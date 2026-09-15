package corpus.generic;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/** Legal twins of {@link GenericAssignBad}: wildcards and target typing. */
public class GenericAssignOk {
    void covariantWildcards(List<String> strings) {
        List<? extends Object> objects = strings;
        List<?> any = strings;
        Collection<String> collection = strings;
    }

    void contravariantWildcard(List<Number> numbers) {
        List<? super Integer> sink = numbers;
        Object first = sink.get(0);
    }

    void diamondsInferredFromTheTarget() {
        List<Number> numbers = new ArrayList<>();
        Map<String, List<Integer>> nested = new HashMap<>();
    }

    void genericMethodsInferredFromTheTarget() {
        List<Integer> numbers = Collections.emptyList();
        List<Object> objects = Collections.singletonList("x");
    }

    void optionalOfASubtype() {
        Optional<Number> number = Optional.of(1);
    }

    void iterableOfTheSameElement(ArrayList<String> strings) {
        Iterable<String> iterable = strings;
    }

    List<? extends Object> returnThroughAWildcard(List<String> strings) {
        return strings;
    }
}
