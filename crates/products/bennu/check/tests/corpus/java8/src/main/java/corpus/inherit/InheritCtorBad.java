package corpus.inherit;

/** Subclasses of a class that has only a parameterized constructor. Twin: {@link InheritCtorOk}. */
public class InheritCtorBad {
    static class WithoutAConstructor extends InheritTypes.NeedsName { // error: compiler.err.cant.apply.symbol
    }

    static class NoArgConstructorWithoutASuperCall extends InheritTypes.NeedsName {
        NoArgConstructorWithoutASuperCall() { } // error: compiler.err.cant.apply.symbol
    }

    static class SuperCallWithTheWrongType extends InheritTypes.NeedsName {
        SuperCallWithTheWrongType() { super(1); } // error: compiler.err.cant.apply.symbol
    }

    void anonymousSubclassWithoutArguments() {
        Object named = new InheritTypes.NeedsName() { }; // error: compiler.err.cant.apply.symbol
    }
}
