package corpus.imports;

import static java.lang.Integer.*;
import static java.lang.Integer.MAX_VALUE;
import static java.lang.Long.*;

/** Legal twin of {@link ImportStaticJdkAmbiguousBad}. */
public class ImportStaticJdkAmbiguousOk {
    void singleStaticImportWins() {
        long max = MAX_VALUE;
    }

    void onlyOneClassHasTheMember() {
        int parsed = parseInt("1");
        long parsedLong = parseLong("1");
    }

    void mostSpecificOverloadAcrossBothClasses() {
        String hex = toHexString(1);
    }

    void qualifiedName() {
        Object parsed = Long.valueOf("1");
    }
}
