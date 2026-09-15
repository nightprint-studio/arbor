package corpus.inherit;

/** Classes and interfaces in the wrong supertype clause. Twin: {@link InheritKindOk}. */
public class InheritKindBad {
    static class ExtendsAnInterface extends Runnable { // error: compiler.err.no.intf.expected.here
    }

    static class ImplementsAClass implements InheritTypes.OpenBase { // error: compiler.err.intf.expected.here
    }

    interface ExtendsAClass extends InheritTypes.OpenBase { // error: compiler.err.intf.expected.here
    }

    static class ImplementsAnAbstractClass implements InheritTypes.Shape { // error: compiler.err.intf.expected.here
    }

    enum EnumImplementsAClass implements InheritTypes.OpenBase { // error: compiler.err.intf.expected.here
        ONLY
    }
}
