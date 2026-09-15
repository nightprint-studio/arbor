package corpus.generic;

/** Legal twins of {@link GenericStaticBad}. */
public class GenericStaticOk<T> {
    T instanceField;

    static <U> U staticGenericMethod(U value) {
        return value;
    }

    T instanceMethod(T value) {
        return value;
    }

    static GenericStaticOk<String> staticFactory() {
        return new GenericStaticOk<String>();
    }

    class InnerClassUsesTheOuterParameter {
        T value;
    }

    static class NestedClassDeclaresItsOwn<T> {
        T value;
    }
}
