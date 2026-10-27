package corpus.syntax;

public class ParseLocalEnum {
    void run() {
        enum Color { RED } // error: compiler.err.local.enum
    }
}
