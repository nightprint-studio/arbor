package corpus.imports.lib;

/** Clean members the import cases reach through every import form. Must stay error-free. */
public final class ImportLib {
    private ImportLib() {
    }

    public static final int LIMIT = 3;

    public static int twice(int value) {
        return value * 2;
    }

    public static String join(String left, String right) {
        return left + right;
    }

    public static class Gadget {
        public Gadget(String name) {
        }

        public void spin(int times) {
        }
    }

    public enum Mode {
        ON, OFF
    }

    public interface Callback {
        void call(String text);
    }
}
