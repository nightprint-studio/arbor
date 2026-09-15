package corpus.inherit;

/** Concrete classes that leave an abstract method unimplemented. Twin: {@link InheritAbstractOk}. */
public class InheritAbstractBad {
    static class MissesEveryInterfaceMethod implements InheritTypes.Greeter { // error: compiler.err.does.not.override.abstract
    }

    static class MissesOneInterfaceMethod implements InheritTypes.Greeter { // error: compiler.err.does.not.override.abstract
        public void greet(String name) {
        }
    }

    static class MissesTheAbstractClassMethods extends InheritTypes.Shape { // error: compiler.err.does.not.override.abstract
    }

    static class OverloadsInsteadOfOverriding implements InheritTypes.Greeter { // error: compiler.err.does.not.override.abstract
        public void greet(Object name) {
        }

        public String name() {
            return "";
        }
    }

    static class DeclaresItsOwnAbstractMethod { // error: compiler.err.does.not.override.abstract
        abstract void unfinished();
    }

    void anonymousClassMissingTheMethod() {
        Runnable task = new Runnable() { }; // error: compiler.err.does.not.override.abstract
    }

    void anonymousClassWithTheWrongSignature() {
        Runnable task = new Runnable() { public void run(int times) { } }; // error: compiler.err.does.not.override.abstract
    }

    void anonymousAbstractClassMissingOneMethod() {
        Object shape = new InheritTypes.Shape() { protected double area() { return 0; } }; // error: compiler.err.does.not.override.abstract
    }
}
