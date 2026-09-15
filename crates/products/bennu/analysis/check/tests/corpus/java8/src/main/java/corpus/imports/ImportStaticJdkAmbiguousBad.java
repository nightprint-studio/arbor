package corpus.imports;

import static java.lang.Integer.*;
import static java.lang.Long.*;

/** Static members both {@code Integer.*} and {@code Long.*} provide with the same signature. Twin: {@link ImportStaticJdkAmbiguousOk}. */
public class ImportStaticJdkAmbiguousBad {
    void ambiguousConstant() {
        long max = MAX_VALUE; // error: compiler.err.ref.ambiguous
    }

    void ambiguousMethodWithIdenticalSignatures() {
        Object parsed = valueOf("1"); // error: compiler.err.ref.ambiguous
    }
}
