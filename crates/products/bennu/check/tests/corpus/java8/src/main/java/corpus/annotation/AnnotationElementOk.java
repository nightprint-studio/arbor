package corpus.annotation;

import corpus.annotation.AnnotationTypes.Marker;
import corpus.annotation.AnnotationTypes.Named;

/** Legal twins of {@link AnnotationElementBad}. */
public class AnnotationElementOk {
    @Named("x")
    void shorthandValue() {
    }

    @Named(value = "x", priority = 1)
    void namedElements() {
    }

    @Named(value = "x", type = String.class, level = AnnotationTypes.Level.HIGH, tags = { "a", "b" })
    void everyElement() {
    }

    @Named(value = "x", tags = "single")
    void singleElementArrayShorthand() {
    }

    @Marker
    void marker() {
    }

    @Marker()
    void markerWithParentheses() {
    }

    @SuppressWarnings({ "unchecked", "rawtypes" })
    void jdkArrayValue() {
    }
}
