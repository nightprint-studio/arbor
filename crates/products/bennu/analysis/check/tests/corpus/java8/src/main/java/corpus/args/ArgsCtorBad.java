package corpus.args;

/** Constructor calls, anonymous classes and constructor chaining with bad arguments. Twin: {@link ArgsCtorOk}. */
public class ArgsCtorBad {
    static class NoExplicitConstructor {
    }

    static class Pair {
        Pair(String left, String right) {
        }
    }

    abstract static class Task {
        Task(String name) {
        }

        abstract void run();
    }

    interface Callback {
        void call();
    }

    class Inner {
        Inner(int size) {
        }
    }

    static class ChildWithBadSuperCall extends Pair {
        ChildWithBadSuperCall() {
            super("left"); // error: compiler.err.cant.apply.symbol
        }
    }

    static class ChildWithBadThisCall extends Pair {
        ChildWithBadThisCall() {
            this(1, 2, 3); // error: compiler.err.cant.apply.symbols
        }

        ChildWithBadThisCall(String name) {
            super(name, name);
        }
    }

    static class ChildWithoutConstructor extends Pair { // error: compiler.err.cant.apply.symbol
    }

    void implicitConstructorGivenArguments() {
        Object created = new NoExplicitConstructor(1); // error: compiler.err.cant.apply.symbol
    }

    void tooFewConstructorArguments() {
        new Pair("left"); // error: compiler.err.cant.apply.symbol
    }

    void wrongConstructorArgumentType() {
        new Pair("left", 1); // error: compiler.err.cant.apply.symbol
    }

    void overloadedConstructorsNoneApplies() {
        new ArgsApi(1); // error: compiler.err.cant.apply.symbols
    }

    void overloadedConstructorsSwapped() {
        new ArgsApi(1, "name"); // error: compiler.err.cant.apply.symbols
    }

    void anonymousSubclassWrongArguments() {
        new Task(1) { void run() { } }; // error: compiler.err.cant.apply.symbol
    }

    void anonymousSubclassMissingArguments() {
        new Task() { void run() { } }; // error: compiler.err.cant.apply.symbol
    }

    void anonymousInterfaceWithArguments() {
        new Callback(1) { public void call() { } }; // error: compiler.err.anon.class.impl.intf.no.args
    }

    void innerClassWrongArgument() {
        new Inner("x"); // error: compiler.err.cant.apply.symbol
    }

    void qualifiedInnerClassMissingArgument(ArgsCtorBad outer) {
        outer.new Inner(); // error: compiler.err.cant.apply.symbol
    }

    void jdkOverloadedConstructors() {
        new StringBuilder(new Object()); // error: compiler.err.cant.apply.symbols
    }

    void jdkNoArgConstructorGivenArguments() {
        new Object(1); // error: compiler.err.cant.apply.symbol
    }
}
