package corpus.statics.holder;

/** A static nested class whose own statics are imported by their full, nested path. */
public final class Registry {
    public static final class Entry {
        public static final String DEFAULT_KEY = "key";

        private final String key;

        private Entry(String key) {
            this.key = key;
        }

        public static Entry of(String key) {
            return new Entry(key);
        }

        public String key() {
            return key;
        }
    }
}
