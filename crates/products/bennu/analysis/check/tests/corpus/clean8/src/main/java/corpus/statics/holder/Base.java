package corpus.statics.holder;

/** Statics a subclass in another package reads bare, with no import at all: it inherits them. */
public class Base {
    public static final String ID = "base";
    protected static final int SHARED = 7;

    protected static int twice(int value) {
        return value * 2;
    }
}
