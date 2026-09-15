package corpus.generic;

import java.util.List;
import java.util.Map;

/** Type parameters and generic types that do not exist. Twin: {@link GenericUnresolvedOk}. */
public class GenericUnresolvedBad<T> {
    U undeclaredTypeParameterAsAField; // error: compiler.err.cant.resolve.location

    static class ExtendsAnUnknownGenericType extends MissingBase<String> { // error: compiler.err.cant.resolve.location
    }

    void undeclaredTypeParameterInAParameter(List<U> values) { // error: compiler.err.cant.resolve.location
    }

    <K> void misspelledMethodTypeParameter(Map<K, V> values) { // error: compiler.err.cant.resolve.location
    }

    void unknownGenericType() {
        MissingBox<String> box = null; // error: compiler.err.cant.resolve.location
    }

    void unknownTypeArgumentOfAKnownType() {
        Map<String, MissingValue> values = null; // error: compiler.err.cant.resolve.location
    }
}
