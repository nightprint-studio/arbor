package corpus.cast;

import java.util.List;
import java.util.Map;

/** Legal twins of {@link CastBoxingBad}; unboxing a null compiles (it fails at run time). */
public class CastBoxingOk {
    void takesLong(long value) {
    }

    void takesIntegerBoxed(Integer value) {
    }

    static void pick(long value) {
    }

    static void pick(Integer value) {
    }

    void boxingToTheMatchingWrapper() {
        Long big = 1L;
        Integer number = 1;
        Double real = 1.0;
    }

    void boxingOfNarrowableConstants() {
        Byte small = 1;
        Short medium = 1;
        Character letter = 65;
    }

    void unboxingThenWidening(Integer number) {
        long big = number;
        double real = number;
    }

    void boxingThenWideningToAReference() {
        Number number = 1;
        Object object = 1L;
        Comparable<Integer> comparable = 1;
    }

    void unboxingANullCompiles() {
        Integer missing = null;
        int value = missing;
    }

    void boxedArguments(Integer number) {
        takesLong(number);
        takesIntegerBoxed(1);
    }

    void longValuesForAMapOfLongs(Map<String, Long> totals) {
        totals.put("key", 1L);
        totals.put("other", Long.valueOf(1));
    }

    void longElementForAListOfLongs(List<Long> totals) {
        totals.add(1L);
    }

    void primitiveOverloadForAPrimitive() {
        pick(1);
    }

    void wrapperOverloadForAWrapper() {
        pick(Integer.valueOf(1));
    }

    void arrayToObject() {
        int[] values = new int[1];
        Object anyArray = values;
    }
}
