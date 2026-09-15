package corpus.annotation;

import java.lang.annotation.ElementType;

/** Legal twins of {@link AnnotationDeclBad}: every member type an annotation may declare. */
public class AnnotationDeclOk {
    static final String CONSTANT = "x";

    @interface EveryLegalMemberType {
        int number();

        long big();

        double real();

        boolean flag();

        char letter();

        byte small();

        String text();

        Class<?> anyType();

        Class<? extends Number> numberType();

        ElementType kind();

        Deprecated nested();

        String[] texts();

        int[] numbers();

        Class<?>[] types();
    }

    @interface WithDefaults {
        int value() default 1 + 2;

        String text() default CONSTANT;

        String[] texts() default {};

        ElementType kind() default ElementType.TYPE;

        Class<?> type() default void.class;
    }

    @interface WithConstantAndNestedType {
        int MAX = 3;

        enum Mode {
            A, B
        }

        int limit() default MAX;

        Mode mode() default Mode.A;
    }

    @interface Empty {
    }
}
