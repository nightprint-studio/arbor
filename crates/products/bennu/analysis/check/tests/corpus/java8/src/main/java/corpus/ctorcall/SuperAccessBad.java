package corpus.ctorcall;

/** `super` as a receiver where it cannot be, or on a member it cannot reach. Twin: {@link SuperAccessOk}. */
public class SuperAccessBad {
    abstract static class Base {
        int field;

        abstract void abstractMethod();

        void concrete() {
        }
    }

    static class SuperInAStaticMethod extends Base {
        void abstractMethod() {
        }

        static void helper() {
            super.concrete(); // error: compiler.err.non-static.cant.be.ref
        }
    }

    static class CallsAnAbstractSuperMethod extends Base {
        void abstractMethod() {
            super.abstractMethod(); // error: compiler.err.abstract.cant.be.accessed.directly
        }
    }

    static class ReadsAMissingSuperField extends Base {
        void abstractMethod() {
        }

        int read() {
            return super.missing; // error: compiler.err.cant.resolve
        }
    }

    interface Left {
        default String name() {
            return "left";
        }
    }

    interface Right {
        default String name() {
            return "right";
        }
    }

    static class UnrelatedDefaultsWithoutAnOverride implements Left, Right { // error: compiler.err.types.incompatible
    }

    interface Root {
        default void hello() {
        }
    }

    interface Middle extends Root {
    }

    static class SkipsAnInterface implements Middle {
        public void hello() {
            Root.super.hello(); // error: compiler.err.not.encl.class
        }
    }

    interface Overridden extends Root {
        default void hello() {
        }
    }

    static class CallsAnOverriddenDefault implements Root, Overridden {
        public void hello() {
            Root.super.hello(); // error: compiler.err.illegal.default.super.call
        }
    }
}
