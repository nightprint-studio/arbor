package corpus.ctorcall;

import java.io.IOException;
import java.util.function.Supplier;

/** Legal twins of {@link CtorCallBad}: what may sit inside `super(...)` / `this(...)`, and where. */
public class CtorCallOk {
    static class Base {
        Base() {
        }

        Base(int value) {
        }

        Base(String... names) {
        }

        Base(Supplier<String> supplier) {
        }
    }

    static class Owner {
        static final int CONSTANT = 3;

        static int compute(int seed) {
            return seed * 2;
        }
    }

    static class StaticMembersAreReachableBeforeSuper extends Base {
        static final int LIMIT = 4;

        static int twice(int value) {
            return value * 2;
        }

        StaticMembersAreReachableBeforeSuper(int seed) {
            super(twice(seed) + LIMIT + Owner.compute(seed) + Owner.CONSTANT);
        }
    }

    static class ParametersAndLambdasAreReachableBeforeSuper extends Base {
        ParametersAndLambdasAreReachableBeforeSuper(String name) {
            super(() -> name);
        }

        ParametersAndLambdasAreReachableBeforeSuper(String first, String second) {
            super(first, second);
        }

        ParametersAndLambdasAreReachableBeforeSuper() {
            super(); // a comment after the call is not a statement
        }
    }

    static class CommentThenSuper extends Base {
        CommentThenSuper() {
            // why this constructor exists
            super(1);
        }
    }

    static class ChainThroughThis {
        ChainThroughThis() {
            this(1);
        }

        ChainThroughThis(int value) {
            this(value, value > 0 ? "positive" : "other");
        }

        ChainThroughThis(int value, String label) {
            System.gc();
        }
    }

    static class ANestedStaticObjectAsAnArgument extends Base {
        static class Seed {
        }

        ANestedStaticObjectAsAnArgument() {
            super(new Seed().hashCode());
        }
    }

    static class UsesThisAfterSuper extends Base {
        int own = 1;

        UsesThisAfterSuper() {
            super(1);
            this.own = compute();
            own++;
        }

        int compute() {
            return 2;
        }
    }

    static class Outer {
        class Inner {
        }
    }

    static class QualifiedSuperOfAnInnerClass extends Outer.Inner {
        QualifiedSuperOfAnInnerClass(Outer outer) {
            outer.super();
        }
    }

    enum ChainedEnum {
        SMALL, LARGE(10);

        final int size;

        ChainedEnum() {
            this(1);
        }

        ChainedEnum(int size) {
            this.size = size;
        }
    }

    static class ThrowingBase {
        ThrowingBase() throws IOException {
        }
    }

    static class DeclaresWhatTheImplicitSuperThrows extends ThrowingBase {
        DeclaresWhatTheImplicitSuperThrows() throws IOException {
        }
    }

    static class ExplicitSuperOfAThrowingConstructor extends ThrowingBase {
        ExplicitSuperOfAThrowingConstructor() throws IOException {
            super();
        }
    }

    <T> CtorCallOk(T seed) {
        this(String.valueOf(seed), 1);
    }

    CtorCallOk(String text, int length) {
        super();
    }
}
