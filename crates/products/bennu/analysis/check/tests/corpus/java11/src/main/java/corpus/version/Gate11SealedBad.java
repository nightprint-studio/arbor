package corpus.version;

/** A sealed type below Java 17. Kept apart: how javac recovers from `sealed` is the least certain gate. */
public class Gate11SealedBad {
    sealed interface Shape { // error: compiler.err.feature.not.supported.in.source.plural
    }

    static final class Circle implements Shape {
    }
}
