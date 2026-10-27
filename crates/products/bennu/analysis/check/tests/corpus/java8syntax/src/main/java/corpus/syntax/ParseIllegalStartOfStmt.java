package corpus.syntax;

public class ParseIllegalStartOfStmt {
    void run() { public int local = 1; } } // error: compiler.err.illegal.start.of.expr compiler.err.expected3
