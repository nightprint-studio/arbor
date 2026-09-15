package corpus.inherit;

/** Illegal modifiers and modifier combinations. Twin: {@link InheritModifierOk}. */
public class InheritModifierBad {
    abstract static final class AbstractAndFinal { // error: compiler.err.illegal.combination.of.modifiers
    }

    final interface FinalInterface { // error: compiler.err.illegal.combination.of.modifiers
    }

    abstract enum AbstractEnum { // error: compiler.err.mod.not.allowed.here
        ONLY
    }

    abstract int abstractField; // error: compiler.err.mod.not.allowed.here

    transient void transientMethod() { // error: compiler.err.mod.not.allowed.here
    }

    volatile final int volatileAndFinal = 1; // error: compiler.err.illegal.combination.of.modifiers

    void methodWithoutABody(); // error: compiler.err.missing.meth.body.or.decl.abstract

    abstract static class Holder {
        private abstract void privateAndAbstract(); // error: compiler.err.illegal.combination.of.modifiers

        static abstract void staticAndAbstract(); // error: compiler.err.illegal.combination.of.modifiers

        final abstract void finalAndAbstract(); // error: compiler.err.illegal.combination.of.modifiers

        abstract void abstractWithABody() { // error: compiler.err.abstract.meth.cant.have.body
        }
    }

    interface Api {
        protected void protectedInterfaceMethod(); // error: compiler.err.mod.not.allowed.here

        final void finalInterfaceMethod(); // error: compiler.err.mod.not.allowed.here

        void bodyWithoutDefault() { // error: compiler.err.intf.meth.cant.have.body
        }
    }
}

private class InheritModifierBadPrivateTopLevel { // error: compiler.err.mod.not.allowed.here
}

static class InheritModifierBadStaticTopLevel { // error: compiler.err.mod.not.allowed.here
}
