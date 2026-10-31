package corpus.annotation;

/** Unknown elements on an annotation read from a class file, not from the project's sources. */
public class AnnotationLibraryElementBad {
    @SuppressWarnings(value = "unchecked", required = true) // error: compiler.err.cant.resolve.location.args
    void valueThenUnknownElement() {
    }

    @Deprecated(required = true) // error: compiler.err.cant.resolve.location.args
    void unknownElementAlone() {
    }
}
