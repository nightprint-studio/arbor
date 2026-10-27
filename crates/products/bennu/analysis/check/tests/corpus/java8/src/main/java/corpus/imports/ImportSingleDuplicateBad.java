package corpus.imports;

import java.awt.List;
import java.util.List; // error: compiler.err.already.defined.single.import

/** Two single-type imports of the same simple name, and an import a local type shadows. Twin: {@link ImportSingleDuplicateOk}. */
public class ImportSingleDuplicateBad {
    List<String> names; // error: compiler.err.ref.ambiguous
}
