package corpus.imports;

import static corpus.imports.lib.ImportLib.Gadget;
import static corpus.imports.lib.ImportLib.LIMIT;
import static corpus.imports.lib.ImportLib.join;
import static corpus.imports.lib.ImportLib.twice;

/** Legal twins of {@link ImportSingleStaticBad}, plus a nested type brought in by a single static import. */
public class ImportSingleStaticOk {
    void rightArgumentType() {
        twice(1);
    }

    void constantOfTheRightType() {
        int limit = LIMIT;
        long widened = LIMIT;
    }

    void importedMember() {
        join("a", "b");
    }

    void staticallyImportedNestedType() {
        Gadget gadget = new Gadget("name");
        gadget.spin(LIMIT);
    }
}
