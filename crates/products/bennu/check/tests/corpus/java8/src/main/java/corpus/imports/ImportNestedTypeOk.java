package corpus.imports;

import corpus.imports.lib.ImportLib.Gadget;
import corpus.imports.lib.ImportLib.Mode;
import java.util.Map.Entry;

/** Legal twins of {@link ImportNestedTypeBad}. */
public class ImportNestedTypeOk {
    void projectNestedTypeConstructor() {
        new Gadget("name");
    }

    void projectNestedTypeMethod(Gadget gadget) {
        gadget.spin(1);
    }

    void projectNestedEnum() {
        Mode mode = Mode.ON;
    }

    void jdkNestedTypeMethods(Entry<String, Integer> entry) {
        String key = entry.getKey();
        entry.setValue(1);
    }
}
