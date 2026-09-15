package corpus.annotation;

import corpus.annotation.AnnotationTypes.Marker;
import corpus.annotation.AnnotationTypes.Named;

/** Annotation elements that do not exist, are missing, or are given twice. Twin: {@link AnnotationElementOk}. */
public class AnnotationElementBad {
    @Named(value = "x", nope = 1) // error: compiler.err.cant.resolve.location.args
    void unknownElement() {
    }

    @Named(value = "x", Priority = 1) // error: compiler.err.cant.resolve.location.args
    void elementNameWithTheWrongCase() {
    }

    @Marker(value = "x") // error: compiler.err.cant.resolve.location.args
    void namedElementOnAMarker() {
    }

    @Marker("x") // error: compiler.err.cant.resolve.location.args
    void shorthandValueOnAMarker() {
    }

    @Named(priority = 1) // error: compiler.err.annotation.missing.default.value
    void requiredElementMissing() {
    }

    @Named(value = "x", value = "y") // error: compiler.err.duplicate.annotation.member.value
    void elementGivenTwice() {
    }

    @Named("a") @Named("b") // error: compiler.err.duplicate.annotation.missing.container
    void notRepeatable() {
    }
}
