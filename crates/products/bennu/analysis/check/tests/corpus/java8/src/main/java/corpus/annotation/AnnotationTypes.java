package corpus.annotation;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/** Clean annotation types shared by the annotation cases. Must stay error-free. */
public final class AnnotationTypes {
    private AnnotationTypes() {
    }

    public enum Level {
        LOW, HIGH
    }

    @Target(ElementType.METHOD)
    public @interface MethodOnly {
    }

    @Target({ ElementType.TYPE, ElementType.FIELD })
    public @interface TypeOrField {
    }

    @Retention(RetentionPolicy.RUNTIME)
    public @interface Named {
        String value();

        int priority() default 0;

        Class<?> type() default Object.class;

        Level level() default Level.LOW;

        String[] tags() default {};
    }

    public @interface Marker {
    }
}
