package corpus.decl;

import java.lang.annotation.ElementType;
import java.lang.annotation.Target;

/** The legal shapes of what {@link DeclMoreBad} gets wrong. */
public class DeclMoreOk {
    enum Color { RED, GREEN }

    static {
        int first = 1;
    }

    static {
        int first = 2;
    }

    @FunctionalInterface
    interface OneMethod {
        void only();
        default void extra() { }
        boolean equals(Object other);
    }

    @Target({ElementType.TYPE, ElementType.METHOD})
    @interface Both { }

    static class GenericBase<T> {
        void accept(T value) { }
    }

    static class Matching extends GenericBase<String> {
        @Override
        void accept(String value) { }
    }

    int unqualifiedLabel(Color color) {
        switch (color) {
            case RED:
                return 1;
            default:
                return 0;
        }
    }

    void localClass() {
        class Local { }
        new Local();
    }
}
