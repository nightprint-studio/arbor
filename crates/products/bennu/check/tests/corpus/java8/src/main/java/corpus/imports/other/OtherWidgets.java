package corpus.imports.other;

/** The twin of {@code corpus.imports.lib.LibWidgets}: the same member names, in another class. Must stay error-free. */
public final class OtherWidgets {
    private OtherWidgets() {
    }

    public static final int VALUE = 2;

    public static int shared(int value) {
        return -value;
    }

    public static class Widget {
        public void fromOther() {
        }
    }
}
