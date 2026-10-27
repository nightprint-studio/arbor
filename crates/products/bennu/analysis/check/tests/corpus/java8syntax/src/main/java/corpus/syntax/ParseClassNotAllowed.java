package corpus.syntax;

public class ParseClassNotAllowed {
    void run(boolean flag) {
        if (flag) class Local { } // error: compiler.err.class.not.allowed
    }
}
