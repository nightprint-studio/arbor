package corpus.imports;

import static corpus.imports.lib.ImportLib.LIMIT;
import static corpus.imports.lib.ImportLib.missingMember; // error: compiler.err.cant.resolve.location
import static corpus.imports.lib.ImportLib.twice;

/** Members reached through single static imports, used wrongly. Twin: {@link ImportSingleStaticOk}. */
public class ImportSingleStaticBad {
    void wrongArgumentType() {
        twice("x"); // error: compiler.err.cant.apply.symbol
    }

    void wrongArgumentCount() {
        twice(1, 2); // error: compiler.err.cant.apply.symbol
    }

    void constantOfTheWrongType() {
        String text = LIMIT; // error: compiler.err.prob.found.req
    }

    void memberThatWasNotImported() {
        join("a", "b"); // error: compiler.err.cant.resolve.location.args
    }
}
