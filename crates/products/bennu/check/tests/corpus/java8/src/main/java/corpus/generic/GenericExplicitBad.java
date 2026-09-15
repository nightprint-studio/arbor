package corpus.generic;

import java.util.Collections;

/** Explicit type arguments that conflict with the call. Twin: {@link GenericExplicitOk}. */
public class GenericExplicitBad {
    <T> void one(T value) {
    }

    static <T extends Number> void numeric(T value) {
    }

    void explicitArgumentConflictsWithTheValue() {
        Collections.<Integer>singletonList("x"); // error: compiler.err.cant.apply.symbol
    }

    void explicitArgumentOutsideTheBound() {
        GenericExplicitBad.<String>numeric("x"); // error: compiler.err.cant.apply.symbol.noargs
    }

    void tooManyExplicitArguments() {
        this.<String, String>one("x"); // error: compiler.err.cant.apply.symbol.noargs
    }

    void primitiveExplicitArgument() {
        this.<int>one(1); // error: compiler.err.type.found.req compiler.err.cant.apply.symbol.noargs
    }
}
