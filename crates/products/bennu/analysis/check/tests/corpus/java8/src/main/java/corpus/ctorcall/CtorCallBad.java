package corpus.ctorcall;

import java.io.IOException;

/** Explicit constructor invocations in the wrong place, with the wrong target, or reading `this` too early. Twin: {@link CtorCallOk}. */
public class CtorCallBad {
    static class Base {
        Base() {
        }

        Base(int value) {
        }
    }

    static class SuperAfterAStatement extends Base {
        SuperAfterAStatement() {
            System.gc();
            super(1); // error: compiler.err.call.must.be.first.stmt.in.ctor
        }
    }

    static class ThisAfterAStatement {
        ThisAfterAStatement() {
            System.gc();
            this(1); // error: compiler.err.call.must.be.first.stmt.in.ctor
        }

        ThisAfterAStatement(int value) {
        }
    }

    static class TwoCalls extends Base {
        TwoCalls() { super(); super(1); } // error: compiler.err.call.must.be.first.stmt.in.ctor
    }

    static class SuperInAMethod extends Base {
        void run() {
            super(1); // error: compiler.err.call.must.be.first.stmt.in.ctor
        }
    }

    static class RecursiveConstructors {
        RecursiveConstructors() { this(1); } RecursiveConstructors(int value) { this(); } // error: compiler.err.recursive.ctor.invocation
    }

    static class ReadsAFieldBeforeSuper extends Base {
        int own = 1;

        ReadsAFieldBeforeSuper() {
            super(own); // error: compiler.err.cant.ref.before.ctor.called
        }
    }

    static class CallsAnInstanceMethodBeforeSuper extends Base {
        int compute() {
            return 1;
        }

        CallsAnInstanceMethodBeforeSuper() {
            super(compute()); // error: compiler.err.cant.ref.before.ctor.called
        }
    }

    static class UsesThisBeforeSuper extends Base {
        UsesThisBeforeSuper() {
            super(this.hashCode()); // error: compiler.err.cant.ref.before.ctor.called
        }
    }

    enum SuperInAnEnumConstructor {
        FIRST;

        SuperInAnEnumConstructor() {
            super(); // error: compiler.err.call.to.super.not.allowed.in.enum.ctor
        }
    }

    static class QualifiedSuperOfAStaticClass extends Base {
        QualifiedSuperOfAStaticClass(Base outer) { outer.super(); } // error: compiler.err.illegal.qual.not.icls
    }

    static class ThrowingBase {
        ThrowingBase() throws IOException {
        }
    }

    static class ImplicitSuperThrows extends ThrowingBase { // error: compiler.err.unreported.exception.default.constructor
    }

    static class ExplicitConstructorHidesTheImplicitSuperThrow extends ThrowingBase {
        ExplicitConstructorHidesTheImplicitSuperThrow() { } // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }
}
