package corpus.imports;

import java.awt.*;
import java.sql.*;
import java.util.*;
import java.util.Date;
import java.util.List;

/** Legal twin of {@link ImportJdkAmbiguousBad}: single-type imports shadow wildcards; unique names resolve through them. */
public class ImportJdkAmbiguousOk {
    void singleTypeImportsWin() {
        List<String> names = new ArrayList<>();
        Date today = new Date();
    }

    void qualifiedNames() {
        java.awt.List widget = null;
        java.sql.Date day = null;
    }

    void namesOnlyOneWildcardProvides() {
        Map<String, Integer> counts = new HashMap<>();
        Connection connection = null;
        Color color = Color.RED;
    }
}
