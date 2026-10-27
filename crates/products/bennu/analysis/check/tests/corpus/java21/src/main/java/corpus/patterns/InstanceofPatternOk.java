package corpus.patterns;

import java.util.Collection;
import java.util.List;

/** Legal twins of {@link InstanceofPatternBad}: where a pattern binding is, and is not, in scope. */
public class InstanceofPatternOk {
    record Point(int x, int y) {
    }

    String name = "field";

    boolean negatedTestEscapesTheIf(Object value) {
        if (!(value instanceof String text)) {
            return false;
        }
        return text.isEmpty();
    }

    boolean andChain(Object value) {
        return value instanceof String text && text.length() > 2 && text.charAt(0) == 'a';
    }

    boolean orWithANegatedTest(Object value) {
        return !(value instanceof String text) || text.isEmpty();
    }

    int bindingEscapesAWhileLoop(Object value) {
        while (!(value instanceof String text)) {
            value = "fallback";
        }
        return text.length();
    }

    String bindingInATernary(Object value) {
        return value instanceof String text ? text : "none";
    }

    boolean recordDeconstruction(Object value) {
        return value instanceof Point(int x, int y) && x == y;
    }

    boolean aParameterizedPatternThatIsChecked(Collection<String> names) {
        return names instanceof List<String> list && list.isEmpty();
    }

    boolean aWildcardPattern(Object value) {
        return value instanceof List<?> list && !list.isEmpty();
    }

    boolean aPatternOfTheSameTypeIsAllowedFromJava21(String value) {
        return value instanceof String text && text.isEmpty();
    }

    boolean aBindingMayShadowAField(Object value) {
        return value instanceof String name && name.isEmpty();
    }

    String aFinalBinding(Object value) {
        if (value instanceof final String text) {
            return text;
        }
        return "";
    }
}
