package corpus.cast;

/** Legal twins of {@link CastLossyBad}: widening, explicit narrowing, and implicit narrowing of constants. */
public class CastLossyOk {
    static final int SMALL_CONSTANT = 10;

    void constantsInRange() {
        byte small = 127;
        short medium = -32768;
        char letter = 65;
    }

    void charConstantArithmetic() {
        char next = 'a' + 1;
    }

    void constantVariables() {
        byte small = SMALL_CONSTANT;
        final int local = 20;
        byte fromLocal = local;
    }

    void compoundAssignmentNarrowsImplicitly(byte small) {
        small += 1000;
        small++;
    }

    void wideningConversions(int value, long big, float real) {
        long widened = value;
        float fromLong = big;
        double fromFloat = real;
    }

    void explicitNarrowing(long big, double real) {
        int whole = (int) big;
        float single = (float) real;
    }

    void charLiteralToByte() {
        byte small = 'a';
    }

    int widenedReturn(short small) {
        return small;
    }
}
