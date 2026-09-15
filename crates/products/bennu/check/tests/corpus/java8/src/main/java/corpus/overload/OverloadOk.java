package corpus.overload;

import java.io.Serializable;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Objects;

/**
 * Legal twins of {@link OverloadBad}, plus overloads settled by the phase they apply in (strict,
 * then loose, then varargs) or by most-specific ranking.
 */
public class OverloadOk {
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

    static void longOrBoxed(long value) {
    }

    static void longOrBoxed(Integer value) {
    }

    static void objectOrVarargs(Object value) {
    }

    static void objectOrVarargs(int... values) {
    }

    static void stringOrObject(String value) {
    }

    static void stringOrObject(Object value) {
    }

    static void intOrCharacter(int value) {
    }

    static void intOrCharacter(Character value) {
    }

    static void listOrCollection(List<String> values) {
    }

    static void listOrCollection(Collection<String> values) {
    }

    static void fixedOrVarargs(String first, String second) {
    }

    static void fixedOrVarargs(String... all) {
    }

    static void boxedOrObject(Integer value) {
    }

    static void boxedOrObject(Object value) {
    }

    void strictPhasePicksTheExactPair() {
        boxedPair(1, Integer.valueOf(1));
    }

    void strictPhasePicksTheWidenedPair() {
        widenedPair(1, 1.0);
    }

    void arrayPicksTheMatchingVarargs() {
        varargsKinds(new int[] { 1, 2 });
    }

    void castSettlesTheNull() {
        nullTarget((String) null);
    }

    void castSettlesTheSupertype() {
        numberOrComparable((Number) 1);
        serializableOrText((CharSequence) "text");
    }

    void strictWideningBeatsBoxing() {
        longOrBoxed(1);
    }

    void boxingBeatsVarargs() {
        objectOrVarargs(1);
    }

    void mostSpecificForNull() {
        stringOrObject(null);
    }

    void charWidensBeforeItBoxes() {
        intOrCharacter('a');
    }

    void mostSpecificCollection() {
        listOrCollection(new ArrayList<String>());
    }

    void fixedArityBeatsVarargs() {
        fixedOrVarargs("a", "b");
    }

    void mostSpecificAfterBoxing() {
        boxedOrObject(1);
    }

    void jdkCharOverload() {
        new StringBuilder().append('c').append(1.5f).append((Object) null);
    }

    void jdkRemoveByIndexOrByValue(List<Integer> values) {
        values.remove(0);
        values.remove(Integer.valueOf(1));
    }

    void jdkNumericOverloads() {
        long biggest = Math.max(1, 2L);
        int rounded = Math.round(1.5f);
    }

    void jdkNullPicksTheArrayOverload() {
        String.valueOf((char[]) null);
    }

    void jdkNullToTheMostSpecificOverload() {
        String text = String.valueOf(null);
    }

    void jdkCastsSettleTheNull() {
        System.out.println((Object) null);
        System.out.println((String) null);
    }

    void jdkVarargsWithPrimitiveArray() {
        List<int[]> wrapped = Arrays.asList(new int[] { 1 });
        int hash = Objects.hash(1, "a");
    }
}
