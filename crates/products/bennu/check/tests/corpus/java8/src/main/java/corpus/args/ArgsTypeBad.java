package corpus.args;

/** Calls whose argument TYPES bind to no parameter. Twin: {@link ArgsTypeOk}. */
public class ArgsTypeBad {
    void takesInt(int value) {
    }

    void takesString(String value) {
    }

    void takesByte(byte value) {
    }

    void takesChar(char value) {
    }

    void takesLongBoxed(Long value) {
    }

    void takesInteger(Integer value) {
    }

    void takesStringArray(String[] values) {
    }

    void takesRunnable(Runnable task) {
    }

    void stringToInt() {
        takesInt("1"); // error: compiler.err.cant.apply.symbol
    }

    void intToString() {
        takesString(1); // error: compiler.err.cant.apply.symbol
    }

    void longToInt() {
        takesInt(1L); // error: compiler.err.cant.apply.symbol
    }

    void doubleToInt() {
        takesInt(1.5); // error: compiler.err.cant.apply.symbol
    }

    void booleanToInt() {
        takesInt(true); // error: compiler.err.cant.apply.symbol
    }

    void constantDoesNotNarrowInAnInvocation() {
        takesByte(1); // error: compiler.err.cant.apply.symbol
    }

    void intConstantToChar() {
        takesChar(65); // error: compiler.err.cant.apply.symbol
    }

    void intDoesNotBoxToLong() {
        takesLongBoxed(1); // error: compiler.err.cant.apply.symbol
    }

    void charDoesNotBoxToInteger() {
        takesInteger('a'); // error: compiler.err.cant.apply.symbol
    }

    void nullToPrimitive() {
        takesInt(null); // error: compiler.err.cant.apply.symbol
    }

    void objectToString() {
        takesString(new Object()); // error: compiler.err.cant.apply.symbol
    }

    void objectArrayToStringArray() {
        takesStringArray(new Object[0]); // error: compiler.err.cant.apply.symbol
    }

    void stringToFunctionalInterface() {
        takesRunnable("task"); // error: compiler.err.cant.apply.symbol
    }

    void swappedArguments(ArgsApi api) {
        api.two(1, "a"); // error: compiler.err.cant.apply.symbol
    }

    void numericConditionalPromotesToLong(boolean flag) {
        takesInt(flag ? 1 : 2L); // error: compiler.err.cant.apply.symbol
    }

    void referenceConditionalWithABadBranch(boolean flag) {
        takesString(flag ? "a" : 1); // error: compiler.err.cant.apply.symbol
    }
}
