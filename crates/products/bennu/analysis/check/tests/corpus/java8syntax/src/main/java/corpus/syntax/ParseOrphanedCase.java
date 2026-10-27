package corpus.syntax;

public class ParseOrphanedCase {
    void run(int a) {
        case 1: break; // error: compiler.err.orphaned
    }
}
