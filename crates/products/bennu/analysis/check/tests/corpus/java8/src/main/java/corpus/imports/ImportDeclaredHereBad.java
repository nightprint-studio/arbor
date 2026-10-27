package corpus.imports;

import java.util.Deque; // error: compiler.err.already.defined.this.unit

/** An import of a name the same file declares. */
public class ImportDeclaredHereBad {
    Object entries;
}

class Deque { }
