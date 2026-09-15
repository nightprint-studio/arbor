package corpus.imports;

import corpus.imports.lib.*;
import corpus.imports.lib.ImportLib.*;

/** Legal twins of {@link ImportOnDemandBad}: names a wildcard import legally resolves. */
public class ImportOnDemandOk {
    void qualifiedStaticCall() {
        ImportLib.twice(1);
    }

    void nestedTypeThroughTheNestedWildcard() {
        new Gadget("name");
    }

    void nestedTypeThroughItsOuterType() {
        new ImportLib.Gadget("name");
    }

    void typesInThePackage() {
        LibWidgets.Widget widget = new LibWidgets.Widget();
        widget.fromLib();
    }

    void nestedEnumThroughTheNestedWildcard() {
        Mode mode = Mode.OFF;
    }

    void nestedInterfaceCall(Callback callback) {
        callback.call("x");
    }
}
