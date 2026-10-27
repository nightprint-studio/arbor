package corpus.syntax;

public class ParseFinallyWithoutTry {
    void run() {
        finally { } // error: compiler.err.finally.without.try
    }
}
