package corpus.imports;

import static corpus.imports.lib.LibWidgets.*;
import static corpus.imports.other.OtherWidgets.*;

/** Names two static on-demand imports both provide. Twins: {@link ImportAmbiguousOk}, {@link ImportMemberShadowOk}. */
public class ImportAmbiguousBad {
    void ambiguousConstant() {
        int value = VALUE; // error: compiler.err.ref.ambiguous
    }

    void ambiguousMethod() {
        shared(1); // error: compiler.err.ref.ambiguous
    }

    void ambiguousNestedType() {
        Widget widget = null; // error: compiler.err.ref.ambiguous
    }
}
