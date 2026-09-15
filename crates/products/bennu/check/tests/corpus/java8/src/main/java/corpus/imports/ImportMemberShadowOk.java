package corpus.imports;

import static corpus.imports.lib.LibWidgets.*;
import static corpus.imports.other.OtherWidgets.*;

/** Legal twin of {@link ImportAmbiguousBad}: the class's own members shadow both on-demand imports. */
public class ImportMemberShadowOk {
    static final int VALUE = 3;

    static int shared(int value) {
        return value;
    }

    static class Widget {
    }

    void ownMembersWin() {
        int value = VALUE;
        shared(1);
        Widget widget = new Widget();
    }
}
