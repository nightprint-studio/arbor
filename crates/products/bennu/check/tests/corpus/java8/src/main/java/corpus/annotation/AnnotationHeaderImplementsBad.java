package corpus.annotation;

/** `implements` on an annotation type is a parse error: alone in its file so parser recovery cannot reach other cases. */
public class AnnotationHeaderImplementsBad {
    @interface ImplementsAnInterface implements Runnable { } // error: compiler.err.expected
}
