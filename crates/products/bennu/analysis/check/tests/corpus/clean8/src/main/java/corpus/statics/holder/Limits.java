package corpus.statics.holder;

/**
 * A class of nothing but public statics: the constants, the helpers and the one mutable counter a
 * client reaches through a static import, bare and by the class name alike.
 */
public final class Limits {
    public static final int MAX_RETRIES = 3;
    public static final String PREFIX = "retry-";
    public static int counter = 0;

    public static int clamp(int value) {
        return Math.max(0, Math.min(value, MAX_RETRIES));
    }

    public static String label(int attempt) {
        return PREFIX + attempt;
    }

    public static <T> T firstNonNull(T first, T second) {
        return first != null ? first : second;
    }

    public static int next() {
        counter++;
        return counter;
    }
}
