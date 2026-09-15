package corpus.generic;

import java.util.Collections;

/** Legal twins of {@link GenericExplicitBad}. */
public class GenericExplicitOk {
    <T> void one(T value) {
    }

    static <T extends Number> void numeric(T value) {
    }

    void explicitArgumentMatchesTheValue() {
        Collections.<String>singletonList("x");
    }

    void explicitArgumentWithinTheBound() {
        GenericExplicitOk.<Integer>numeric(1);
    }

    void explicitArgumentWiderThanTheValue() {
        this.<Object>one("x");
    }

    void explicitArgumentOnANonGenericMethodIsIgnored(String text) {
        text.<String>length();
    }
}
