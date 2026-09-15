package corpus.imports.shadow;

import static corpus.imports.lib.LibWidgets.*;
import static corpus.imports.other.OtherWidgets.*;

/** A same-package type shadows the two on-demand imports that would otherwise make {@code Widget} ambiguous. */
public class ImportPackageShadowOk {
    void samePackageTypeWins() {
        Widget widget = new Widget();
        widget.local();
    }
}
