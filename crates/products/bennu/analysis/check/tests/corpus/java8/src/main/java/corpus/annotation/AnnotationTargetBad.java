package corpus.annotation;

import corpus.annotation.AnnotationTypes.MethodOnly;
import corpus.annotation.AnnotationTypes.TypeOrField;

/** Annotations placed where their @Target does not allow them. Twin: {@link AnnotationTargetOk}. */
@MethodOnly // error: compiler.err.annotation.type.not.applicable
public class AnnotationTargetBad {
    @MethodOnly // error: compiler.err.annotation.type.not.applicable
    private int field;

    @TypeOrField // error: compiler.err.annotation.type.not.applicable
    void method() {
    }

    void parameter(@MethodOnly int value) { // error: compiler.err.annotation.type.not.applicable
    }

    void localVariable() {
        @TypeOrField // error: compiler.err.annotation.type.not.applicable
        int value = 1;
    }

    @Override // error: compiler.err.annotation.type.not.applicable
    private int overrideOnAField;

    @FunctionalInterface // error: compiler.err.annotation.type.not.applicable
    void functionalInterfaceOnAMethod() {
    }
}
