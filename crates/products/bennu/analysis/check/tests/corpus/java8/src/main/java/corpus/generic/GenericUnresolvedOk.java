package corpus.generic;

import java.util.List;
import java.util.Map;

/** Legal twins of {@link GenericUnresolvedBad}. */
public class GenericUnresolvedOk<T> {
    T declaredTypeParameterAsAField;

    static class ExtendsAKnownGenericType extends GenericApi.Box<String> {
    }

    class InnerClassUsesTheOuterParameter {
        List<T> values;
    }

    void declaredTypeParameterInAParameter(List<T> values) {
    }

    <K, V> void methodTypeParameters(Map<K, V> values) {
    }

    <S extends T> void methodTypeParameterBoundByTheClassParameter(S value) {
    }
}
