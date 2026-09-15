package corpus.annotation;

import java.lang.annotation.Annotation;

/** Legal twin of {@link AnnotationHeaderBad}: no supertype clause, and a class may implement an annotation interface. */
public class AnnotationHeaderOk {
    @interface Plain {
    }

    static class ImplementsAnAnnotationInterface implements Plain {
        public Class<? extends Annotation> annotationType() {
            return Plain.class;
        }
    }
}
