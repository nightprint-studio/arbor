package corpus.equals;

import java.io.Serializable;
import java.util.List;

/** Legal {@code .equals()} calls that look suspicious but are not impossible.
 *  Twin: {@link ImpossibleEqualsBad}. */
public class ImpossibleEqualsOk {

    enum Color { RED, GREEN, BLUE }

    static class Alpha {}
    static class Beta {}

    // Same type — not disjoint.
    void sameString(String a, String b) {
        a.equals(b);
    }

    // Same enum type — not disjoint.
    void sameEnum(Color a, Color b) {
        a.equals(b);
    }

    // Receiver is Object — erased, the check skips it.
    void objectReceiver(Object obj, String text) {
        obj.equals(text);
    }

    // Argument is Object — erased, the check skips it.
    void objectArg(String text, Object obj) {
        text.equals(obj);
    }

    // Receiver is an interface — any class could implement it; skip.
    void interfaceReceiver(Serializable a, String b) {
        a.equals(b);
    }

    // Receiver is a parameterized interface — skip.
    void listReceiver(List<String> a, List<Integer> b) {
        a.equals(b);
    }

    // Both non-final, no subtype relation — a subclass could bridge them.
    void bothNonFinal(Alpha a, Beta b) {
        a.equals(b);
    }

    // Integer is a subtype of Number — subtype_verdict(Integer, Number) != Some(false).
    void supertypeReceiver(Number a, Integer b) {
        a.equals(b);
    }

    // Number is abstract but not final; Integer extends Number — not disjoint.
    void subtypeArg(Integer a, Number b) {
        a.equals(b);
    }

    // A primitive argument is boxed to its own wrapper — same type as the receiver, or a supertype of it.
    void boxedSameWrapper(Integer a, int b, Long c, long d) {
        a.equals(b);
        c.equals(d);
    }

    void boxedIntoSupertype(Number a, int b, Object c, long d) {
        a.equals(b);
        c.equals(d);
    }

    // No explicit receiver — the check requires an object field; implicit this is skipped.
    boolean noReceiver(Object arg) {
        return equals(arg);
    }
}
