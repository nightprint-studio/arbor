package corpus.generics;

import java.lang.annotation.Annotation;

/**
 * A type variable named like a class some OTHER file declares (test fixtures called {@code A}, {@code D}...)
 * is still the variable: the project-wide simple-name guess must not bind it to that class.
 */
public class TypeVariableNames<D extends Runnable> {

    D delegate;

    static <A extends Annotation> A find(String name, Class<A> type) {
        return null;
    }

    Annotation annotationOf() {
        Annotation found = find("x", Annotation.class);
        return found;
    }

    void runDelegate() {
        delegate.run();
    }
}
