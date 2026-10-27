package corpus.syntax;

public class ParseTryWithoutCatch {
    void run() {
        try { } // error: compiler.err.try.without.catch.finally.or.resource.decls
    }
}
