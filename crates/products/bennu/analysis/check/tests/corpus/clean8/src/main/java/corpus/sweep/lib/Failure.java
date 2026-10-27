package corpus.sweep.lib;

/** A public constructor beside protected and package-private ones: a caller elsewhere can only use the public one. */
public class Failure extends Exception {
    protected Failure() {
    }

    protected Failure(String message) {
        super(message);
    }

    public Failure(Throwable cause) {
        super(cause);
    }

    Failure(String[] hidden, ClassLoader loader) {
    }

    public Failure(String[] shown, Object loader) {
    }
}
