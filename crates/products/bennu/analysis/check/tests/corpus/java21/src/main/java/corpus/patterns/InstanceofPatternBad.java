package corpus.patterns;

import java.util.List;

/** `instanceof` patterns: scope of the binding, redeclaration, incompatible and unchecked types. Twin: {@link InstanceofPatternOk}. */
public class InstanceofPatternBad {
    boolean bindingOutOfScopeInAnOrCondition(Object value) {
        return value instanceof String text || text.isEmpty(); // error: compiler.err.cant.resolve.location
    }

    void bindingRedeclared(Object value) {
        String text = "";
        if (value instanceof String text) { // error: compiler.err.already.defined
            System.gc();
        }
    }

    boolean incompatibleTypes(String value) {
        return value instanceof Integer number; // error: compiler.err.prob.found.req
    }

    void uncheckedParameterizedPattern(Object value) {
        if (value instanceof List<String> names) { // error: compiler.err.instanceof.reifiable.not.safe
            System.gc();
        }
    }

    int bindingUsedAfterAnIfThatDoesNotEscape(Object value) {
        if (value instanceof String text) {
            System.gc();
        }
        return text.length(); // error: compiler.err.cant.resolve.location
    }

    void bindingUsedInTheElseOfAPositiveTest(Object value) {
        if (value instanceof String text) {
            System.gc();
        } else {
            System.out.println(text); // error: compiler.err.cant.resolve.location
        }
    }
}
