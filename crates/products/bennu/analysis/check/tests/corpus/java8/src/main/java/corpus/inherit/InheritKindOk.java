package corpus.inherit;

/** Legal twins of {@link InheritKindBad}. */
public class InheritKindOk {
    static class ImplementsAnInterface implements Runnable {
        public void run() {
        }
    }

    static class ExtendsAnAbstractClass extends InheritTypes.Shape {
        protected double area() {
            return 0;
        }

        public String label() {
            return "";
        }
    }

    interface ExtendsInterfaces extends Runnable, InheritTypes.Greeter {
    }

    enum EnumImplementsAnInterface implements Runnable {
        ONLY;

        public void run() {
        }
    }

    static class ExtendsAndImplements extends InheritTypes.OpenBase implements Runnable {
        public void run() {
        }
    }
}
