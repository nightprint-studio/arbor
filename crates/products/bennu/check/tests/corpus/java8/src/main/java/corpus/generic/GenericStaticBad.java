package corpus.generic;

/** A class type parameter referenced from a static context. Twin: {@link GenericStaticOk}. */
public class GenericStaticBad<T> {
    static T staticField; // error: compiler.err.non-static.cant.be.ref

    static T staticMethodReturn() { // error: compiler.err.non-static.cant.be.ref
        return null;
    }

    static void staticMethodParameter(T value) { // error: compiler.err.non-static.cant.be.ref
    }

    static void staticMethodLocal() {
        T local = null; // error: compiler.err.non-static.cant.be.ref
    }

    static class NestedClassUsesTheOuterParameter {
        T value; // error: compiler.err.non-static.cant.be.ref
    }
}
