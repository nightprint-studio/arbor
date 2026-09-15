package corpus.generic;

import java.util.ArrayList;
import java.util.List;

/**
 * Uses of generic types that need them reified: `new T()`, generic arrays, `T.class`, primitive type
 * arguments, generic `instanceof`, generic exceptions. Twin: {@link GenericReifyOk}.
 *
 * The generic `instanceof` must stay the only one in this module: javac reports the level-8 error once
 * per compilation and then treats later occurrences under the level-16 rule.
 */
public class GenericReifyBad<T> {
    static class GenericException<X> extends Exception { // error: compiler.err.generic.throwable
    }

    T instantiateATypeVariable() {
        return new T(); // error: compiler.err.type.found.req
    }

    T[] arrayOfATypeVariable() {
        return new T[10]; // error: compiler.err.generic.array.creation
    }

    Object arrayOfAParameterizedType() {
        return new List<String>[3]; // error: compiler.err.generic.array.creation
    }

    Object classLiteralOfATypeVariable() {
        return T.class; // error: compiler.err.type.var.cant.be.deref
    }

    boolean instanceofAParameterizedType(Object value) {
        return value instanceof List<String>; // error: compiler.err.feature.not.supported.in.source.plural
    }

    void primitiveTypeArgument() {
        List<int> values = null; // error: compiler.err.type.found.req
    }

    void primitiveTypeArgumentInACreation() {
        Object values = new ArrayList<boolean>(); // error: compiler.err.type.found.req
    }
}
