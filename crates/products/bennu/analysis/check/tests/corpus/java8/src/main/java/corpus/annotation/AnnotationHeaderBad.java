package corpus.annotation;

/** An annotation type may not declare a supertype. Alone in its file, so any recovery stays here. Twin: {@link AnnotationHeaderOk}. */
public class AnnotationHeaderBad {
    @interface ExtendsAnInterface extends Runnable { } // error: compiler.err.cant.extend.intf.annotation
}
