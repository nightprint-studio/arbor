package corpus.cast;

import java.io.Serializable;
import java.util.List;

/** Legal twins of {@link CastInconvertibleBad}: every cast the language accepts. */
public class CastInconvertibleOk {
    static class Open {
    }

    void downcastsFromObject(Object value) {
        String text = (String) value;
        Integer number = (Integer) value;
    }

    void nonFinalClassToAnInterface(Open open) {
        Runnable task = (Runnable) open;
    }

    void interfaceToInterface(Runnable task) {
        Comparable<String> comparable = (Comparable<String>) task;
    }

    void primitiveNarrowing(double real) {
        int whole = (int) real;
        char letter = (char) 65;
        byte small = (byte) 300;
    }

    void boxingAndUnboxingCasts(Object value, int number) {
        int unboxed = (int) (Integer) value;
        Object boxed = (Object) number;
        Integer exact = (Integer) number;
    }

    void castToAWildcardType(Object value) {
        List<?> list = (List<?>) value;
    }

    void castOfNull() {
        String text = (String) null;
    }

    void arrayCasts(Object value, String[] texts) {
        Object[] objects = texts;
        String[] back = (String[]) objects;
        int[] numbers = (int[]) value;
    }

    void castToTheSameType(String text) {
        String same = (String) text;
    }

    void intersectionCastOfALambda() {
        Runnable task = (Runnable & Serializable) () -> { };
    }
}
