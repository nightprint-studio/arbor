package corpus.version;

/**
 * Declarations and expressions newer than Java 11, once each. Text blocks are not here: javac reports
 * them on the line of the opening delimiter, which can carry no trailing marker comment (the unit tests
 * of {@code source/version.rs} cover that gate). Twin: {@link Gate11Ok}.
 */
public class Gate11SyntaxBad {
    record Point(int x, int y) { // error: compiler.err.feature.not.supported.in.source.plural compiler.warn.restricted.type.not.allowed.preview
    } // error: compiler.err.missing.ret.stmt

    boolean instanceofPattern(Object value) {
        return value instanceof String text && !text.isEmpty(); // error: compiler.err.feature.not.supported.in.source
    }
}
