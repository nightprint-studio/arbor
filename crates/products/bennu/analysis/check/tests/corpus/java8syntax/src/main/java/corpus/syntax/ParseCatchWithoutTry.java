package corpus.syntax;

public class ParseCatchWithoutTry {
    void run() {
        catch (RuntimeException e) { } // error: compiler.err.catch.without.try
    }
}
