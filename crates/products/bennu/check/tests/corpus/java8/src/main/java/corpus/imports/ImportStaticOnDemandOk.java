package corpus.imports;

import static corpus.imports.lib.ImportLib.*;

/** Legal twins of {@link ImportStaticOnDemandBad}. */
public class ImportStaticOnDemandOk {
    void rightArgumentType() {
        twice(1);
    }

    void rightArgumentCount() {
        join("a", "b");
    }

    void constantOfTheRightType() {
        int limit = LIMIT;
    }

    void nestedTypeConstructor() {
        new Gadget("name").spin(LIMIT);
    }

    void nestedEnumConstant() {
        Mode mode = Mode.ON;
    }

    void nestedInterfaceLambda() {
        Callback callback = text -> text.length();
    }
}
