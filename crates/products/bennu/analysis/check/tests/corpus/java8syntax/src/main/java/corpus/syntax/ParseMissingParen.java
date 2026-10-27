package corpus.syntax;

public class ParseMissingParen {
    void run(int a) {
        if (a > 1 { // error: compiler.err.expected
        }
    }
}
