package corpus.args;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Legal twins of {@link ArgsCtorBad}, plus constructor shapes a naive checker misreads. */
public class ArgsCtorOk {
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

    static class ChildWithSuperCall extends Pair {
        ChildWithSuperCall() {
            super("left", "right");
        }
    }

    static class ChildWithThisCall extends Pair {
        ChildWithThisCall() {
            this("name");
        }

        ChildWithThisCall(String name) {
            super(name, name);
        }
    }

    static class ChildOfNoArgParent extends NoExplicitConstructor {
    }

    void implicitConstructor() {
        Object created = new NoExplicitConstructor();
    }

    void exactConstructorArguments() {
        new Pair("left", "right");
    }

    void nullConstructorArguments() {
        new Pair(null, null);
    }

    void overloadedConstructors() {
        new ArgsApi();
        new ArgsApi("name");
        new ArgsApi("name", 1);
    }

    void anonymousSubclass() {
        new Task("name") { void run() { } };
    }

    void anonymousSubclassWithItsOwnHelper() {
        new Task("name") {
            void run() {
                helper();
            }

            void helper() {
            }
        };
    }

    void anonymousInterface() {
        new Callback() { public void call() { } };
    }

    void memberOfAnAnonymousClassCalledOnTheCreation() {
        int extra = new Object() { int extra() { return 1; } }.extra();
    }

    void innerClass() {
        new Inner(1);
        this.new Inner(2);
    }

    void qualifiedInnerClass(ArgsCtorOk outer) {
        outer.new Inner(1);
    }

    void jdkOverloadedConstructors() {
        new StringBuilder("x");
        new StringBuilder(16);
    }

    void charPicksTheIntConstructor() {
        new StringBuilder('c');
    }

    void jdkNoArgConstructor() {
        new Object();
    }

    void genericConstructors() {
        List<String> copy = new ArrayList<String>(Arrays.asList("a"));
        Map<String, Integer> sized = new HashMap<String, Integer>(16, 0.75f);
        List<String> inferred = new ArrayList<>();
    }
}
