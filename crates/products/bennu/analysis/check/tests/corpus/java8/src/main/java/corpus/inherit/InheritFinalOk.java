package corpus.inherit;

/** Legal twins of {@link InheritFinalBad}. */
public class InheritFinalOk {
    static class LocalOpen {
    }

    static class FromAnOpenProjectClass extends InheritTypes.OpenBase {
    }

    static class FromObject extends Object {
    }

    static class FromThread extends Thread {
    }

    static class FromANestedOpenClass extends LocalOpen {
    }

    static class OverridesAnOpenMethod extends InheritTypes.OpenBase {
        @Override
        public void open() {
        }
    }

    void anonymousSubclassOfAnOpenClass() {
        Object created = new InheritTypes.OpenBase() { };
    }

    void finalClassInstantiatedNotSubclassed() {
        InheritTypes.FinalBase instance = new InheritTypes.FinalBase();
    }
}
