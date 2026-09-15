package corpus.inherit;

/** Legal twins of {@link InheritModifierBad}. */
public class InheritModifierOk {
    private static final int PRIVATE_CONSTANT = 1;

    transient volatile int transientAndVolatile;

    abstract static class AbstractNested {
        abstract void unfinished();
    }

    static final class FinalNested {
    }

    interface Api {
        int CONSTANT = 1;

        void implicitlyPublicAbstract();

        public abstract void explicitlyPublicAbstract();

        static void staticInterfaceMethod() {
        }

        default void defaultInterfaceMethod() {
        }
    }

    enum WithConstructor {
        ONLY(1);

        private final int weight;

        WithConstructor(int weight) {
            this.weight = weight;
        }
    }

    synchronized void synchronizedMethod() {
    }

    native void nativeMethod();

    protected static void protectedStaticMethod() {
    }

    void finalLocalsAndParameters(final int parameter) {
        final int local = parameter;
    }

    public static void main(String[] args) {
    }
}

class InheritModifierOkPackagePrivateTopLevel {
}

abstract class InheritModifierOkAbstractTopLevel {
}

final class InheritModifierOkFinalTopLevel {
}
