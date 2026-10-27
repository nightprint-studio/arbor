package corpus.syntax;

public class LexBadUnicodeEscape {
    String text = "\u12"; // error: compiler.err.illegal.unicode.esc
}
