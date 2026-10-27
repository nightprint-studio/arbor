package corpus.imports;

import java.util.List;
import java.util.Map;

/** Distinct simple names, and the same type imported twice (which is legal). Twin of {@link ImportSingleDuplicateBad}. */
public class ImportSingleDuplicateOk {
    List<String> names;
    java.awt.List awt;
    Map<String, String> entries;
}
