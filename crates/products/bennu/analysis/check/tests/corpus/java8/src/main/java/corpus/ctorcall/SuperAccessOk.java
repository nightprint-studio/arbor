package corpus.ctorcall;

import java.util.function.Supplier;

/** Legal twins of {@link SuperAccessBad}. */
public class SuperAccessOk {
    static class Base<T> {
        int field = 1;

        String text() {
            return "base";
        }

        void take(T value) {
        }

        static void staticMethod() {
        }
    }

    static class UsesSuperInEveryInstanceContext extends Base<String> {
        int field = 2;

        @Override
        String text() {
            Supplier<String> viaLambda = () -> super.text();
            Supplier<String> viaMethodReference = super::text;
            Runnable viaAnonymousClass = new Runnable() {
                public void run() {
                    UsesSuperInEveryInstanceContext.super.text();
                }
            };
            super.take("value");
            super.staticMethod();
            return super.text() + super.field + this.field + viaLambda.get() + viaMethodReference.get();
        }

        class Inner {
            String delegate() {
                return UsesSuperInEveryInstanceContext.super.text();
            }
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

    static class ResolvesUnrelatedDefaults implements Left, Right {
        public String name() {
            return Left.super.name() + Right.super.name();
        }
    }

    interface Parent {
        default void run() {
        }
    }

    interface Child extends Parent {
        default void run() {
            Parent.super.run();
        }
    }

    static class Grandchild implements Child {
        public void run() {
            Child.super.run();
        }
    }

    static class DelegatesToObject {
        @Override
        public String toString() {
            return super.toString() + super.hashCode() + super.equals(this);
        }
    }
}
