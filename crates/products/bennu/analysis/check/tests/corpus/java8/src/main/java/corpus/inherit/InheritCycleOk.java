package corpus.inherit;

import java.util.function.IntBinaryOperator;

/** Legal twins of {@link InheritCycleBad}: types that mention themselves without inheriting from themselves. */
public class InheritCycleOk {
    static class Node<T extends Node<T>> {
    }

    static class Leaf extends Node<Leaf> {
    }

    abstract static class Builder<B extends Builder<B>> {
        abstract B self();
    }

    static class ConcreteBuilder extends Builder<ConcreteBuilder> {
        ConcreteBuilder self() {
            return this;
        }
    }

    interface Ordered<T extends Ordered<T>> {
        int compareTo(T other);
    }

    static class Ranked implements Ordered<Ranked>, Comparable<Ranked> {
        public int compareTo(Ranked other) {
            return 0;
        }
    }

    static class Enclosing {
        class InnerExtendingItsEnclosingClass extends Enclosing {
        }
    }

    static class Chain1 {
    }

    static class Chain2 extends Chain1 {
    }

    static class Chain3 extends Chain2 {
    }

    interface Top {
    }

    interface Left extends Top {
    }

    interface Right extends Top {
    }

    interface Bottom extends Left, Right {
    }

    enum Operation implements IntBinaryOperator {
        ADD {
            public int applyAsInt(int left, int right) {
                return left + right;
            }
        }
    }
}
