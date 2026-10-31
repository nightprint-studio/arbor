package corpus.overloads;

import java.lang.annotation.Annotation;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.reflect.Constructor;
import java.lang.reflect.Type;
import java.lang.reflect.TypeVariable;
import java.util.Collections;
import java.util.Map;

/**
 * Shapes Apache Commons Lang compiles and Bennu once refused: legal code, so any diagnostic is a false
 * positive.
 */
public class CommonsLangShapes {

    @Retention(RetentionPolicy.RUNTIME)
    @interface Tag {
        String value();
    }

    /** An annotation type is an {@code Annotation}, and a class implementing it overrides {@code annotationType}. */
    static class TagImpl implements Tag {
        @Override
        public Class<? extends Annotation> annotationType() {
            return Tag.class;
        }

        @Override
        public String value() {
            return "v";
        }
    }

    static boolean same(Annotation a, Annotation b) {
        return a.equals(b);
    }

    static int hash(Annotation a) {
        return a.hashCode();
    }

    void annotationsArePassedAsAnnotation(Tag tag) {
        same(tag, new TagImpl());
        hash(tag);
    }

    // `int[]` is no `Object[]`, so only the first of the two can take a primitive array.
    static boolean containsAny(int[] array, int... toFind) {
        return array != null && toFind.length > 0;
    }

    static boolean containsAny(Object[] array, Object... toFind) {
        return array != null && toFind.length > 0;
    }

    void primitiveArraysAreNotObjectArrays(int[] array) {
        containsAny((int[]) null, 1);
        containsAny(array, 0);
    }

    // Private overloads of another top-level class are no candidates from here.
    void privateOverloadsAreNotCandidates(Type type) {
        Types.equal(null, type);
    }

    // A generic method call passed to a parameter with type arguments is typed by that parameter.
    <T> void callsAreTypedByTheirTarget(TypeVariable<?> variable, Type type) {
        Types.unroll(Collections.emptyMap(), type);
        Types.unroll(Collections.singletonMap(variable, String.class), type);
    }

    // A for-each variable is another binding than a `final` local of the same name elsewhere.
    Constructor<?> forEachVariableIsNotTheFinalLocal(Constructor<?>[] all, boolean direct) {
        if (direct) {
            final Constructor<?> ctor = all[0];
            return ctor;
        }
        for (Constructor<?> ctor : all) {
            ctor = all[all.length - 1];
            return ctor;
        }
        return null;
    }
}

class Types {
    public static boolean equal(Type a, Type b) {
        return a == b;
    }

    private static boolean equal(Class<?> a, Type b) {
        return a == b;
    }

    private static boolean equal(Constructor<?> a, Type b) {
        return false;
    }

    static Type unroll(Map<TypeVariable<?>, Type> arguments, Type type) {
        return arguments.getOrDefault(type, type);
    }
}
