package corpus.args;

/** Legal twins of {@link ArgsTypeBad}, plus every conversion an invocation context allows. */
public class ArgsTypeOk {
    void takesInt(int value) {
    }

    void takesLong(long value) {
    }

    void takesFloat(float value) {
    }

    void takesDouble(double value) {
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

    void takesObject(Object value) {
    }

    void takesNumber(Number value) {
    }

    void takesCharSequence(CharSequence value) {
    }

    void takesComparable(Comparable<String> value) {
    }

    void takesStringArray(String[] values) {
    }

    void takesObjectArray(Object[] values) {
    }

    void takesRunnable(Runnable task) {
    }

    void exactPrimitive() {
        takesInt(1);
    }

    void exactReference() {
        takesString("1");
    }

    void wideningIntToLong() {
        takesLong(1);
    }

    void wideningIntToDouble() {
        takesDouble(1);
    }

    void wideningLongToFloat() {
        takesFloat(1L);
    }

    void wideningCharToInt() {
        takesInt('A');
    }

    void wideningShortToInt(short value) {
        takesInt(value);
    }

    void promotedExpression() {
        takesInt((short) 1 + (byte) 1);
    }

    void castToByte() {
        takesByte((byte) 1);
    }

    void charLiteral() {
        takesChar('A');
    }

    void boxingLong() {
        takesLongBoxed(1L);
    }

    void boxingInt() {
        takesInteger(1);
    }

    void unboxing() {
        takesInt(Integer.valueOf(1));
    }

    void unboxingThenWidening() {
        takesLong(Integer.valueOf(1));
        takesDouble(Integer.valueOf(1));
    }

    void boxingThenWideningReference() {
        takesObject(1);
        takesNumber(1.5);
    }

    void nullToReferences() {
        takesString(null);
        takesInteger(null);
    }

    void arrays() {
        takesObject(new Object[0]);
        takesObjectArray(new String[0]);
        takesStringArray(new String[] { "a" });
    }

    void interfacesImplementedByTheArgument() {
        takesCharSequence(new StringBuilder());
        takesComparable("x");
    }

    void lambdaToFunctionalInterface() {
        takesRunnable(() -> {
        });
    }

    void stringConcatenation() {
        takesString("a" + 1);
    }

    void numericConditionals(boolean flag) {
        takesInt(flag ? 1 : 2);
        takesLong(flag ? 1 : 2L);
    }

    void referenceConditionals(boolean flag) {
        takesObject(flag ? 1 : "a");
        takesString(flag ? "a" : null);
        takesInteger(flag ? 1 : null);
    }

    void conditionalUnboxedToPrimitive(boolean flag) {
        takesInt(flag ? 1 : null);
    }

    void correctOrder(ArgsApi api) {
        api.two("a", 1);
    }
}
