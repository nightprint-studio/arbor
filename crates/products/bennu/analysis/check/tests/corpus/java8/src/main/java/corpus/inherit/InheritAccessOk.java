package corpus.inherit;

/** Legal twins of {@link InheritAccessBad}: overrides keep or widen access; private methods are not overridden. */
public class InheritAccessOk {
    static class PublicInterfaceMethod implements Runnable {
        public void run() {
        }
    }

    static class WidensProtectedToPublic extends InheritTypes.Shape {
        public double area() {
            return 0;
        }

        public String label() {
            return "";
        }
    }

    static class KeepsProtected extends InheritTypes.Shape {
        protected double area() {
            return 0;
        }

        public String label() {
            return "";
        }
    }

    static class PackageBase {
        void work() {
        }

        private void secret() {
        }
    }

    static class WidensPackagePrivateToProtected extends PackageBase {
        protected void work() {
        }

        void secret() {
        }
    }

    void anonymousClassKeepsPublicAccess() {
        Runnable task = new Runnable() { public void run() { } };
    }
}
