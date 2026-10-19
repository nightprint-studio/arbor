package corpus.statics.holder;

import java.util.Locale;

/** An enum whose constants are imported statically and whose static factory is imported with them. */
public enum Level {
    LOW(1),
    MEDIUM(5),
    HIGH(10);

    private final int weight;

    Level(int weight) {
        this.weight = weight;
    }

    public int weight() {
        return weight;
    }

    public static Level parse(String text) {
        return valueOf(text.toUpperCase(Locale.ROOT));
    }
}
