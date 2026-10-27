package corpus.syntax;

public class ParseIllegalDot {
    void run() { Object o = System.out..toString(); } // error: compiler.err.illegal.dot
}
