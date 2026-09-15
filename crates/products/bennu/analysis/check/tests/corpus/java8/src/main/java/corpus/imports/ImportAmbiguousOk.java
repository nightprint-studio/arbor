package corpus.imports;

import corpus.imports.lib.LibWidgets;
import corpus.imports.other.OtherWidgets;

import static corpus.imports.lib.LibWidgets.*;
import static corpus.imports.lib.LibWidgets.VALUE;
import static corpus.imports.lib.LibWidgets.Widget;
import static corpus.imports.lib.LibWidgets.shared;
import static corpus.imports.other.OtherWidgets.*;

/** Legal twin of {@link ImportAmbiguousBad}: a single static import shadows the on-demand ones, and qualified names are never ambiguous. */
public class ImportAmbiguousOk {
    void singleStaticImportsWin() {
        int value = VALUE;
        shared(1);
        Widget widget = new Widget();
        widget.fromLib();
    }

    void qualifiedNames() {
        int other = OtherWidgets.VALUE;
        OtherWidgets.shared(1);
        new OtherWidgets.Widget().fromOther();
        int lib = LibWidgets.VALUE;
    }
}
