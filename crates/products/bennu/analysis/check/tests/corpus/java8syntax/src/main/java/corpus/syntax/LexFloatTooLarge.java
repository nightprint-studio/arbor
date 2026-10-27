package corpus.syntax;

public class LexFloatTooLarge {
    float value = 1e50f; // error: compiler.err.fp.number.too.large
}
