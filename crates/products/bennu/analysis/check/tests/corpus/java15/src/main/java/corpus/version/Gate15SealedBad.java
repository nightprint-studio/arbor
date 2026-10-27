package corpus.version;

/** A sealed type below Java 17. Kept apart: how javac recovers from `sealed` is the least certain gate. */
public class Gate15SealedBad {
    sealed interface Shape { // error: compiler.err.feature.not.supported.in.source.plural
    }

    static final class Circle implements Shape {
    }
}
