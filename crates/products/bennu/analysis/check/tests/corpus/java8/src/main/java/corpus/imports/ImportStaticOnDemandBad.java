package corpus.imports;

import static corpus.imports.lib.ImportLib.*;

/** Members and nested types reached through a static on-demand import, used wrongly. Twin: {@link ImportStaticOnDemandOk}. */
public class ImportStaticOnDemandBad {
    void wrongArgumentType() {
        twice("x"); // error: compiler.err.cant.apply.symbol
    }

    void wrongArgumentCount() {
        join("a"); // error: compiler.err.cant.apply.symbol
    }

    void constantOfTheWrongType() {
        String text = LIMIT; // error: compiler.err.prob.found.req
    }

    void nestedTypeConstructorWrongType() {
        new Gadget(1); // error: compiler.err.cant.apply.symbol
    }

    void nestedEnumUnknownConstant() {
        Mode mode = Mode.DIM; // error: compiler.err.cant.resolve.location
    }

    void memberTheClassDoesNotHave() {
        thrice(1); // error: compiler.err.cant.resolve.location.args
    }

    void nestedInterfaceLambdaWithTheWrongArity() {
        Callback callback = (first, second) -> { }; // error: compiler.err.prob.found.req
    }
}
