package corpus.version;

/** Declarations and expressions newer than Java 15, once each. Twin: {@link Gate15Ok}. */
public class Gate15Bad {
    record Point(int x, int y) { // error: compiler.err.feature.not.supported.in.source.plural compiler.warn.restricted.type.not.allowed.preview
    } // error: compiler.err.missing.ret.stmt

    boolean instanceofPattern(Object value) {
        return value instanceof String text && !text.isEmpty(); // error: compiler.err.feature.not.supported.in.source
    }
}
