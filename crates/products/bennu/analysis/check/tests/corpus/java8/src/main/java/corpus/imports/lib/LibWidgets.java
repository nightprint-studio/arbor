package corpus.imports.lib;

/**
 * Shares every member name with {@code corpus.imports.other.OtherWidgets}, so static on-demand
 * imports of both make each name ambiguous. Must stay error-free.
 */
public final class LibWidgets {
    private LibWidgets() {
    }

    public static final int VALUE = 1;

    public static int shared(int value) {
        return value;
    }

    public static class Widget {
        public void fromLib() {
        }
    }
}
