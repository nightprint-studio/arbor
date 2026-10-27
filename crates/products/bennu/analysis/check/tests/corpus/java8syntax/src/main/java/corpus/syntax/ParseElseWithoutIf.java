package corpus.syntax;

public class ParseElseWithoutIf {
    void run() {
        else { } // error: compiler.err.else.without.if
    }
}
