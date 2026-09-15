package corpus.cast;

/** Narrowing primitive conversions without a cast. Twin: {@link CastLossyOk}. */
public class CastLossyBad {
    void longToInt(long big) {
        int small = big; // error: compiler.err.prob.found.req
    }

    void doubleLiteralToFloat() {
        float real = 1.0; // error: compiler.err.prob.found.req
    }

    void doubleLiteralToInt() {
        int whole = 1.5; // error: compiler.err.prob.found.req
    }

    void longLiteralToInt() {
        int whole = 1L; // error: compiler.err.prob.found.req
    }

    void constantOutOfByteRange() {
        byte small = 128; // error: compiler.err.prob.found.req
    }

    void negativeConstantToChar() {
        char letter = -1; // error: compiler.err.prob.found.req
    }

    void intVariableToShort(int value) {
        short small = value; // error: compiler.err.prob.found.req
    }

    void intVariableToChar(int value) {
        char letter = value; // error: compiler.err.prob.found.req
    }

    void sumOfBytesIsAnInt(byte first, byte second) {
        byte sum = first + second; // error: compiler.err.prob.found.req
    }

    void conditionalOfIntConstantsIsNotAConstant(boolean flag) {
        byte small = flag ? 1 : 2; // error: compiler.err.prob.found.req
    }

    void lossyArrayInitializerElement() {
        int[] values = { 1, 2L }; // error: compiler.err.prob.found.req
    }

    int lossyReturn(long big) {
        return big; // error: compiler.err.prob.found.req
    }
}
