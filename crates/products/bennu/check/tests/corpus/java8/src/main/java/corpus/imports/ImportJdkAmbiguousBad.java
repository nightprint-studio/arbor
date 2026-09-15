package corpus.imports;

import java.awt.*;
import java.sql.*;
import java.util.*;

/** Simple names two JDK package wildcards both provide. Twin: {@link ImportJdkAmbiguousOk}. */
public class ImportJdkAmbiguousBad {
    void listIsInJavaUtilAndJavaAwt() {
        List names = null; // error: compiler.err.ref.ambiguous
    }

    void dateIsInJavaUtilAndJavaSql() {
        Date today = null; // error: compiler.err.ref.ambiguous
    }
}
