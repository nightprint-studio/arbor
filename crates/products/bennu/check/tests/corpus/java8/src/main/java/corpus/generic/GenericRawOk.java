package corpus.generic;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Raw types compile: javac raises unchecked warnings at most (only a note without -Xlint). Nothing
 * here may be reported as an error. No Bad twin: raw use is never an error.
 */
public class GenericRawOk {
    void rawListAcceptsAnything() {
        List raw = new ArrayList();
        raw.add(1);
        raw.add("x");
    }

    void rawAssignedToAParameterizedType() {
        List raw = new ArrayList();
        List<String> typed = raw;
    }

    void parameterizedAssignedToARawType(List<String> typed) {
        List raw = typed;
        raw.add(1);
    }

    void rawMap() {
        Map raw = new HashMap();
        raw.put(1, "x");
        Object value = raw.get(1);
    }

    void rawComparable() {
        Comparable raw = "x";
        int order = raw.compareTo("y");
    }

    void rawIteration(List raw) {
        for (Object element : raw) {
            element.hashCode();
        }
    }

    void rawProjectGenericType() {
        GenericApi.Box raw = new GenericApi.Box();
        raw.set(1);
        Object value = raw.get();
    }

    void uncheckedCastAndRawClassLiteral(Object value) {
        List<String> cast = (List<String>) value;
        Class<List> type = List.class;
    }
}
