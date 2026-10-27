package corpus.syntax;

public class LexIllegalEscape {
    String text = "bad \q escape"; // error: compiler.err.illegal.esc.char
}
