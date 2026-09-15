package corpus.overload;

import java.io.Serializable;

/** Overload resolution that ends in ambiguity or in no applicable candidate. Twin: {@link OverloadOk}. */
public class OverloadBad {
    static void boxedPair(int first, Integer second) {
    }

    static void boxedPair(Integer first, int second) {
    }

    static void widenedPair(int first, double second) {
    }

    static void widenedPair(double first, int second) {
    }

    static void varargsKinds(int... values) {
    }

    static void varargsKinds(Integer... values) {
    }

    static void nullTarget(String value) {
    }

    static void nullTarget(StringBuilder value) {
    }

    static void numberOrComparable(Number value) {
    }

    static void numberOrComparable(Comparable<?> value) {
    }

    static void serializableOrText(Serializable value) {
    }

    static void serializableOrText(CharSequence value) {
    }

    void ambiguousAfterBoxing() {
        boxedPair(1, 1); // error: compiler.err.ref.ambiguous
    }

    void ambiguousAfterWidening() {
        widenedPair(1, 1); // error: compiler.err.ref.ambiguous
    }

    void ambiguousPrimitiveAndBoxedVarargs() {
        varargsKinds(1, 2); // error: compiler.err.ref.ambiguous
    }

    void ambiguousNull() {
        nullTarget(null); // error: compiler.err.ref.ambiguous
    }

    void ambiguousBoxedIntegerIsBothSupertypes() {
        numberOrComparable(1); // error: compiler.err.ref.ambiguous
    }

    void ambiguousStringIsBothInterfaces() {
        serializableOrText("text"); // error: compiler.err.ref.ambiguous
    }

    void ambiguousJdkNull() {
        System.out.println(null); // error: compiler.err.ref.ambiguous
    }

    void noCandidateApplies() {
        boxedPair("a", "b"); // error: compiler.err.cant.apply.symbols
    }

    void longBoxesToNeitherCandidate() {
        boxedPair(1L, 1); // error: compiler.err.cant.apply.symbols
    }

    void doubleWidensToNeitherCandidate() {
        widenedPair(1.0, 1.0); // error: compiler.err.cant.apply.symbols
    }
}
