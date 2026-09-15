package corpus.imports;

import corpus.imports.lib.*;
import corpus.imports.lib.ImportLib.*;

/** Types reached through type-import-on-demand (package and nested types), used wrongly. Twin: {@link ImportOnDemandOk}. */
public class ImportOnDemandBad {
    void qualifiedStaticCallWrongType() {
        ImportLib.twice("x"); // error: compiler.err.cant.apply.symbol
    }

    void nestedTypeThroughTheNestedWildcard() {
        new Gadget(1); // error: compiler.err.cant.apply.symbol
    }

    void nestedTypeThroughItsOuterType() {
        new ImportLib.Gadget(1); // error: compiler.err.cant.apply.symbol
    }

    void typeNotInThePackage() {
        NotInLib value = null; // error: compiler.err.cant.resolve.location
    }

    void staticMemberIsNotImportedByATypeWildcard() {
        twice(1); // error: compiler.err.cant.resolve.location.args
    }

    void nestedInterfaceCallWrongType(Callback callback) {
        callback.call(1); // error: compiler.err.cant.apply.symbol
    }
}
