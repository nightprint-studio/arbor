package corpus.statics.holder;

/**
 * Interface constants are implicitly {@code public static final}, and a static interface method is
 * reached through the interface only: it is not inherited by an implementing class.
 */
public interface Defaults {
    int TIMEOUT = 30;
    String NAME = "default";

    static String describe() {
        return NAME + ":" + TIMEOUT;
    }
}
