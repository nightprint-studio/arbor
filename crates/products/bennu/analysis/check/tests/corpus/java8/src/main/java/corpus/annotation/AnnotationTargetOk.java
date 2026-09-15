package corpus.annotation;

import java.util.List;

import corpus.annotation.AnnotationTypes.MethodOnly;
import corpus.annotation.AnnotationTypes.TypeOrField;

/** Legal twins of {@link AnnotationTargetBad}. */
@TypeOrField
public class AnnotationTargetOk {
    @TypeOrField
    private int field;

    @MethodOnly
    void method() {
    }

    @Override
    public String toString() {
        return "";
    }

    @Deprecated
    void untargetedAnnotationsFitAnyDeclaration(@Deprecated int value) {
        @SuppressWarnings("unused")
        int local = value;
    }

    @FunctionalInterface
    interface Task {
        void run();
    }

    @SafeVarargs
    final void safeVarargsOnAFinalMethod(List<String>... lists) {
    }
}
