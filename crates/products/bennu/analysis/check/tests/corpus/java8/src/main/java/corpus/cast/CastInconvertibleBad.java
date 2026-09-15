package corpus.cast;

/** Casts between inconvertible types, and assignments of incompatible types. Twin: {@link CastInconvertibleOk}. */
public class CastInconvertibleBad {
    void boxedIntegerToString(Integer value) {
        Object result = (String) value; // error: compiler.err.prob.found.req
    }

    void stringToBoxedInteger(String text) {
        Object result = (Integer) text; // error: compiler.err.prob.found.req
    }

    void stringToPrimitive(String text) {
        int result = (int) text; // error: compiler.err.prob.found.req
    }

    void intToBoolean(int value) {
        boolean result = (boolean) value; // error: compiler.err.prob.found.req
    }

    void intToString(int value) {
        Object result = (String) value; // error: compiler.err.prob.found.req
    }

    void finalClassToAnInterfaceItDoesNotImplement(String text) {
        Object result = (Runnable) text; // error: compiler.err.prob.found.req
    }

    void finalClassToAnUnrelatedClass(StringBuilder builder) {
        Object result = (Thread) builder; // error: compiler.err.prob.found.req
    }

    void differentPrimitiveArrays(int[] values) {
        Object result = (long[]) values; // error: compiler.err.prob.found.req
    }

    void intAssignedToAString() {
        String text = 1; // error: compiler.err.prob.found.req
    }

    void objectAssignedToAString(Object value) {
        String text = value; // error: compiler.err.prob.found.req
    }

    void primitiveArrayAssignedToABoxedArray(int[] values) {
        Integer[] boxed = values; // error: compiler.err.prob.found.req
    }
}
