package corpus.args;

/** Calls whose argument COUNT matches no overload. Twin: {@link ArgsCountOk}. */
public class ArgsCountBad {
    void local(int value) {
    }

    void tooMany(ArgsApi api) {
        api.one(1, 2); // error: compiler.err.cant.apply.symbol
    }

    void tooFew(ArgsApi api) {
        api.two("a"); // error: compiler.err.cant.apply.symbol
    }

    void argumentToNoArgMethod(ArgsApi api) {
        api.none(1); // error: compiler.err.cant.apply.symbol
    }

    void noArgumentToOneArgMethod(ArgsApi api) {
        api.one(); // error: compiler.err.cant.apply.symbol
    }

    void staticTooMany() {
        ArgsApi.twice(1, 2); // error: compiler.err.cant.apply.symbol
    }

    void bareTooMany() {
        local(1, 2); // error: compiler.err.cant.apply.symbol
    }

    void varargsMissingLeadingParameter(ArgsApi api) {
        api.leadingThenVarargs(); // error: compiler.err.cant.apply.symbol
    }

    void jdkTooMany(String text) {
        text.isEmpty(1); // error: compiler.err.cant.apply.symbol
    }

    void jdkOverloadedNoneTakesZero(String text) {
        text.substring(); // error: compiler.err.cant.apply.symbols
    }

    void throughThisTooFew() {
        this.local(); // error: compiler.err.cant.apply.symbol
    }
}
