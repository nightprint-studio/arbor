package corpus.syntax;

public class ParseVarargsNotLast {
    void run(int... values, int last) { } // error: compiler.err.varargs.must.be.last
}
