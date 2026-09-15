package corpus.inherit;

/** Subclasses of final classes. Twin: {@link InheritFinalOk}. */
public class InheritFinalBad {
    static final class LocalFinal {
    }

    static class FromAProjectFinalClass extends InheritTypes.FinalBase { // error: compiler.err.cant.inherit.from.final
    }

    static class FromString extends String { // error: compiler.err.cant.inherit.from.final
    }

    static class FromANestedFinalClass extends LocalFinal { // error: compiler.err.cant.inherit.from.final
    }

    void anonymousSubclassOfAFinalClass() {
        Object created = new InheritTypes.FinalBase() { }; // error: compiler.err.cant.inherit.from.final
    }
}
