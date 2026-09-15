package corpus.inherit;

/** Overrides that make a method less visible than the one they override. Twin: {@link InheritAccessOk}. */
public class InheritAccessBad {
    static class PackagePrivateInterfaceMethod implements Runnable {
        void run() { // error: compiler.err.override.weaker.access
        }
    }

    static class ProtectedInterfaceMethod implements InheritTypes.Greeter {
        protected void greet(String name) { // error: compiler.err.override.weaker.access
        }

        public String name() {
            return "";
        }
    }

    static class PackagePrivateOverrideOfAProtectedAbstractMethod extends InheritTypes.Shape {
        double area() { // error: compiler.err.override.weaker.access
            return 0;
        }

        public String label() {
            return "";
        }
    }

    static class PrivateOverrideOfAPublicMethod extends InheritTypes.OpenBase {
        private void open() { // error: compiler.err.override.weaker.access
        }
    }

    static class ProtectedToString {
        protected String toString() { // error: compiler.err.override.weaker.access
            return "";
        }
    }

    void anonymousClassWeakensAccess() {
        Runnable task = new Runnable() { void run() { } }; // error: compiler.err.override.weaker.access
    }
}
