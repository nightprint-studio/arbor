package corpus.inherit;

import java.util.List;
import java.util.Set;
import java.util.function.Function;

/** Legal twins of {@link InheritDuplicateBad}: overloads, distinct erasures, and names in different namespaces. */
public class InheritDuplicateOk {
    int count;

    static class OverloadedByType {
        void run(int value) {
        }

        void run(long value) {
        }

        void run(String value) {
        }
    }

    static class OverloadedByArity {
        void run() {
        }

        void run(int first) {
        }

        void run(int first, int second) {
        }
    }

    static class DifferentErasures {
        void accept(List<String> values) {
        }

        void accept(Set<String> values) {
        }
    }

    static class GenericAndConcreteOverloads {
        <T> void accept(T value) {
        }

        void accept(String value) {
        }
    }

    static class FieldAndMethodShareAName {
        int size;

        int size() {
            return size;
        }
    }

    static class FieldAndNestedTypeShareAName {
        int Inner;

        class Inner {
        }
    }

    static class ConstructorOverloads {
        ConstructorOverloads() {
        }

        ConstructorOverloads(String name) {
        }

        ConstructorOverloads(int size) {
        }
    }

    static class OverloadsAnInheritedMethod extends InheritTypes.OpenBase {
        public void open(int times) {
        }
    }

    void sameNameInSeparateBlocks() {
        {
            int value = 1;
        }
        {
            int value = 2;
        }
    }

    void lambdaParameterNamedLikeAField() {
        Function<String, Integer> length = count -> count.length();
    }
}
