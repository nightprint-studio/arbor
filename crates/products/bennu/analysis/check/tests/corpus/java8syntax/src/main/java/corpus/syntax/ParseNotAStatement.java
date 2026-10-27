package corpus.syntax;

public class ParseNotAStatement {
    void run(int a) {
        a + 1; // error: compiler.err.not.stmt
    }
}
