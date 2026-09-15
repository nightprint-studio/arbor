package corpus.annotation;

import corpus.annotation.AnnotationTypes.Level;
import corpus.annotation.AnnotationTypes.Named;

/** Legal twins of {@link AnnotationValueBad}: every value a constant of the element's type. */
public class AnnotationValueOk {
    static final String CONSTANT = "x";
    static final int BASE = 10;

    @Named(CONSTANT)
    void constantField() {
    }

    @Named("a" + BASE)
    void constantExpression() {
    }

    @Named(value = CONSTANT + "!", priority = BASE * 2)
    void constantArithmetic() {
    }

    @Named(value = "x", tags = { CONSTANT, "literal" })
    void constantArrayElements() {
    }

    @Named(value = "x", level = Level.HIGH)
    void enumConstant() {
    }

    @Named(value = "x", priority = 'a')
    void charWidensToInt() {
    }

    @Named(value = "x", type = int.class)
    void primitiveClassLiteral() {
    }

    @Named(value = "x", priority = (int) 1L)
    void castInAConstantExpression() {
    }

    @Named(value = AnnotationValueOk.CONSTANT)
    void qualifiedConstant() {
    }
}
