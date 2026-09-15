package corpus.generic;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/** Assignments between incompatible parameterizations. Twin: {@link GenericAssignOk}. */
public class GenericAssignBad {
    void listOfStringToListOfObject() {
        List<Object> objects = new ArrayList<String>(); // error: compiler.err.prob.found.req
    }

    void listOfIntegerToListOfNumber(List<Integer> integers) {
        List<Number> numbers = integers; // error: compiler.err.prob.found.req
    }

    void wildcardWithTheWrongBound(List<String> strings) {
        List<? extends Number> numbers = strings; // error: compiler.err.prob.found.req
    }

    void mapWithTheWrongValueType() {
        Map<String, Object> values = new HashMap<String, String>(); // error: compiler.err.prob.found.req
    }

    void superWildcardReadAsItsBound(List<? super Integer> sink) {
        Integer first = sink.get(0); // error: compiler.err.prob.found.req
    }

    void unboundedWildcardToAConcreteList(List<?> any) {
        List<String> strings = any; // error: compiler.err.prob.found.req
    }

    void inferredResultOfTheWrongType() {
        List<Integer> numbers = Collections.singletonList("x"); // error: compiler.err.prob.found.req
    }

    void optionalOfTheWrongType() {
        Optional<String> text = Optional.of(1); // error: compiler.err.prob.found.req
    }

    List<Object> returnOfTheWrongParameterization(List<String> strings) {
        return strings; // error: compiler.err.prob.found.req
    }
}
