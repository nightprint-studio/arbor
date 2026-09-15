package corpus.imports;

import corpus.imports.lib.ImportLib.Gadget;
import corpus.imports.lib.ImportLib.Missing; // error: compiler.err.cant.resolve.location
import java.util.Map.Entry;

/** Nested types reached through single-type imports, used wrongly. Twin: {@link ImportNestedTypeOk}. */
public class ImportNestedTypeBad {
    void projectNestedTypeConstructorWrongType() {
        new Gadget(1); // error: compiler.err.cant.apply.symbol
    }

    void projectNestedTypeMethodWrongType(Gadget gadget) {
        gadget.spin("x"); // error: compiler.err.cant.apply.symbol
    }

    void jdkNestedTypeUnknownMethod(Entry<String, Integer> entry) {
        entry.getKeyValue(); // error: compiler.err.cant.resolve.location.args
    }

    void jdkNestedTypeWrongValueType(Entry<String, Integer> entry) {
        entry.setValue("x"); // error: compiler.err.cant.apply.symbol
    }
}
