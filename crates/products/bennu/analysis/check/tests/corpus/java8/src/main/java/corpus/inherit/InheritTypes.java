package corpus.inherit;

/** Clean supertypes shared by the inheritance cases. Must stay error-free. */
public final class InheritTypes {
    private InheritTypes() {
    }

    public static final class FinalBase {
    }

    public static class OpenBase {
        public final void locked() {
        }

        public void open() {
        }

        public static void helper() {
        }

        public Object produce() {
            return null;
        }
    }

    public interface Greeter {
        void greet(String name);

        String name();
    }

    public abstract static class Shape {
        protected abstract double area();

        public abstract String label();
    }

    public abstract static class NeedsName {
        protected NeedsName(String name) {
        }
    }
}
