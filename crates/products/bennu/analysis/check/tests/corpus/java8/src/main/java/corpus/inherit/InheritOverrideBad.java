package corpus.inherit;

/** Overrides that are illegal, or @Override that overrides nothing. Twin: {@link InheritOverrideOk}. */
public class InheritOverrideBad {
    static class OverrideOfNothing {
        @Override // error: compiler.err.method.does.not.override.superclass
        public void missing() {
        }
    }

    static class OverrideOfAnOverload extends InheritTypes.OpenBase {
        @Override // error: compiler.err.method.does.not.override.superclass
        public void open(int times) {
        }
    }

    static class OverrideOnAStaticMethod extends InheritTypes.OpenBase {
        @Override // error: compiler.err.static.methods.cannot.be.annotated.with.override
        public static void helper() {
        }
    }

    static class OverridesAFinalMethod extends InheritTypes.OpenBase {
        public void locked() { // error: compiler.err.override.meth
        }
    }

    static class IncompatibleReturnType extends InheritTypes.OpenBase {
        public int produce() { // error: compiler.err.override.incompatible.ret
            return 0;
        }
    }

    static class InstanceMethodOverStaticMethod extends InheritTypes.OpenBase {
        public void helper() { // error: compiler.err.override.meth
        }
    }

    static class StaticMethodOverInstanceMethod extends InheritTypes.OpenBase {
        public static void open() { // error: compiler.err.override.static
        }
    }

    static class InterfaceReturnTypeMismatch implements InheritTypes.Greeter { // error: compiler.err.does.not.override.abstract
        public void greet(String name) {
        }

        public Object name() { // error: compiler.err.override.incompatible.ret
            return "";
        }
    }

    static class OverrideDeclaresABroaderCheckedException extends InheritTypes.OpenBase {
        public void open() throws Exception { // error: compiler.err.override.meth.doesnt.throw
        }
    }
}
