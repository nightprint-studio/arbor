package corpus.inherit;

import java.util.Comparator;

/** Legal twins of {@link InheritAbstractBad}, plus abstract methods satisfied from less obvious places. */
public class InheritAbstractOk {
    static class ImplementsEveryInterfaceMethod implements InheritTypes.Greeter {
        public void greet(String name) {
        }

        public String name() {
            return "";
        }
    }

    static class ImplementsTheAbstractClassMethods extends InheritTypes.Shape {
        protected double area() {
            return 0;
        }

        public String label() {
            return "";
        }
    }

    abstract static class StaysAbstract implements InheritTypes.Greeter {
    }

    static class NameProvider {
        public String name() {
            return "";
        }
    }

    static class InheritsOneImplementationFromItsSuperclass extends NameProvider implements InheritTypes.Greeter {
        public void greet(String name) {
        }
    }

    interface NamedByDefault extends InheritTypes.Greeter {
        default String name() {
            return "";
        }
    }

    static class ImplementsTheRestOfADefaultedInterface implements NamedByDefault {
        public void greet(String name) {
        }
    }

    static class ImplementsAGenericInterface implements Comparable<ImplementsAGenericInterface> {
        public int compareTo(ImplementsAGenericInterface other) {
            return 0;
        }
    }

    enum ConstantsImplementTheAbstractMethod {
        FIRST {
            int weight() {
                return 1;
            }
        },
        SECOND {
            int weight() {
                return 2;
            }
        };

        abstract int weight();
    }

    void anonymousClassImplementingEverything() {
        Runnable task = new Runnable() { public void run() { } };
    }

    void lambdaForAnInterfaceThatAlsoDeclaresObjectMethods() {
        Comparator<String> byLength = (left, right) -> left.length() - right.length();
    }
}
