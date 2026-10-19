package corpus.statics.client;

import static corpus.statics.holder.Defaults.NAME;
import static corpus.statics.holder.Defaults.TIMEOUT;
import static corpus.statics.holder.Defaults.describe;
import static corpus.statics.holder.Level.HIGH;
import static corpus.statics.holder.Level.LOW;
import static corpus.statics.holder.Level.parse;
import static corpus.statics.holder.Limits.*;
import static corpus.statics.holder.Registry.Entry.DEFAULT_KEY;
import static corpus.statics.holder.Registry.Entry.of;

import corpus.statics.holder.Base;
import corpus.statics.holder.Level;

import java.util.ArrayList;
import java.util.List;

/**
 * Every way a static member is reached without writing its class: a single static import, an
 * on-demand one, an inherited static, an interface constant, a nested class's static, an enum
 * constant in a {@code switch}, a name that is both a local variable and an imported method.
 */
public class StaticImportClient extends Base {
    private int total;

    public int run(int attempts) {
        int bounded = clamp(attempts);
        total = bounded + MAX_RETRIES + TIMEOUT;
        return total + twice(SHARED);
    }

    public String text(int attempt) {
        return label(attempt) + NAME + describe() + ID;
    }

    public Level pick(boolean strict) {
        return strict ? HIGH : LOW;
    }

    public Level parsed(String text) {
        return parse(text);
    }

    public int weightOf(Level level) {
        switch (level) {
            case LOW:
                return 1;
            case HIGH:
                return 10;
            default:
                return level.weight();
        }
    }

    /** A variable and a method live in different namespaces, so the local does not hide the call. */
    public String shadowed(int attempt) {
        String label = label(attempt);
        return label + describe();
    }

    /** The class's own {@code next()} wins over the one the on-demand import brings in. */
    public int advance() {
        counter = counter + 1;
        return next() + counter;
    }

    public int next() {
        return 42;
    }

    public String entryKey() {
        return of(DEFAULT_KEY).key();
    }

    public List<String> names(String preferred) {
        List<String> names = new ArrayList<>();
        names.add(firstNonNull(preferred, NAME));
        names.add(PREFIX);
        return names;
    }
}
