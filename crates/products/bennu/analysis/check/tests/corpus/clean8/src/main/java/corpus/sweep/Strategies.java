package corpus.sweep;

/** Constants with bodies and generic methods, one of which reaches {@code super}. */
public enum Strategies {
    FIRST() {
        @Override
        <T> boolean put(T object, int hashes) {
            return hashes > 0 && object != null;
        }
    },
    SECOND {
        @Override
        <K> K copy(K key) {
            K fresh = super.copy(key);
            return fresh;
        }

        @Override
        <T> boolean put(T object, int hashes) {
            return false;
        }
    };

    abstract <T> boolean put(T object, int hashes);

    <K> K copy(K key) {
        return key;
    }
}
