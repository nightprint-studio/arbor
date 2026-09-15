package corpus.annotation;

import corpus.annotation.AnnotationTypes.Level;
import corpus.annotation.AnnotationTypes.Named;

/** Annotation element values that are not constants, or not of the element's type. Twin: {@link AnnotationValueOk}. */
public class AnnotationValueBad {
    static String mutable = "x";
    static final String COMPUTED = String.valueOf(1);
    static final Level LEVEL = Level.HIGH;
    static int counter = 1;

    @Named(mutable) // error: compiler.err.attribute.value.must.be.constant
    void nonFinalField() {
    }

    @Named(COMPUTED) // error: compiler.err.attribute.value.must.be.constant
    void finalFieldThatIsNotAConstant() {
    }

    @Named("a" + counter) // error: compiler.err.attribute.value.must.be.constant
    void nonConstantExpression() {
    }

    @Named(value = "x", tags = { mutable }) // error: compiler.err.attribute.value.must.be.constant
    void nonConstantArrayElement() {
    }

    @Named(value = "x", level = LEVEL) // error: compiler.err.enum.annotation.must.be.enum.constant
    void enumConstantThroughAField() {
    }

    @Named(1) // error: compiler.err.prob.found.req
    void intForAString() {
    }

    @Named(value = "x", priority = "high") // error: compiler.err.prob.found.req
    void stringForAnInt() {
    }

    @Named(value = "x", type = "java.lang.String") // error: compiler.err.prob.found.req
    void stringForAClass() {
    }

    @Named(value = "x", level = "HIGH") // error: compiler.err.prob.found.req
    void stringForAnEnum() {
    }

    @Named(value = "x", tags = { 1, 2 }) // error: compiler.err.prob.found.req
    void intsForAStringArray() {
    }

    @Named(value = "x", priority = 1L) // error: compiler.err.prob.found.req
    void lossyLongForAnInt() {
    }
}
