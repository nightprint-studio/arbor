package corpus.inherit;

/** Legal twins of {@link InheritCtorBad}. */
public class InheritCtorOk {
    static class CallsTheSuperConstructor extends InheritTypes.NeedsName {
        CallsTheSuperConstructor() {
            super("name");
        }
    }

    static class DelegatesToItsOwnConstructor extends InheritTypes.NeedsName {
        DelegatesToItsOwnConstructor() {
            this("default");
        }

        DelegatesToItsOwnConstructor(String name) {
            super(name);
        }
    }

    static class ParentWithTwoConstructors {
        ParentWithTwoConstructors() {
        }

        ParentWithTwoConstructors(String name) {
        }
    }

    static class ImplicitSuperFindsTheNoArgConstructor extends ParentWithTwoConstructors {
    }

    void anonymousSubclassWithArguments() {
        Object named = new InheritTypes.NeedsName("name") { };
    }
}
