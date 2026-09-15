package corpus.cast;

import java.util.List;
import java.util.Map;

/** Boxing and unboxing that the language does not perform. Twin: {@link CastBoxingOk}. */
public class CastBoxingBad {
    void takesLongBoxed(Long value) {
    }

    void takesIntegerBoxed(Integer value) {
    }

    void boxedIntegerToBoxedLong(Integer value) {
        Long result = value; // error: compiler.err.prob.found.req
    }

    void boxedLongToBoxedInteger(Long value) {
        Integer result = value; // error: compiler.err.prob.found.req
    }

    void intToBoxedLong() {
        Long result = 1; // error: compiler.err.prob.found.req
    }

    void intToBoxedDouble() {
        Double result = 1; // error: compiler.err.prob.found.req
    }

    void charToBoxedInteger() {
        Integer result = 'a'; // error: compiler.err.prob.found.req
    }

    void nullToPrimitive() {
        int value = null; // error: compiler.err.prob.found.req
    }

    void differentPrimitiveArrayTypes() {
        long[] values = new int[1]; // error: compiler.err.prob.found.req
    }

    void boxedIntegerArgumentForABoxedLong(Integer value) {
        takesLongBoxed(value); // error: compiler.err.cant.apply.symbol
    }

    void longArgumentForABoxedInteger() {
        takesIntegerBoxed(1L); // error: compiler.err.cant.apply.symbol
    }

    void intValueForAMapOfLongs(Map<String, Long> totals) {
        totals.put("key", 1); // error: compiler.err.cant.apply.symbol
    }

    void intElementForAListOfLongs(List<Long> totals) {
        totals.add(1); // error: compiler.err.cant.apply.symbols
    }
}
