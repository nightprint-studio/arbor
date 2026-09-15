package corpus.inherit;

import java.util.List;
import java.util.function.Function;

/** Members and locals declared twice, directly or after erasure. Twin: {@link InheritDuplicateOk}. */
public class InheritDuplicateBad {
    static class SameSignature {
        void run(int value) {
        }

        void run(int other) { // error: compiler.err.already.defined
        }
    }

    static class SameSignatureDifferentReturnType {
        void compute() {
        }

        int compute() { // error: compiler.err.already.defined
            return 0;
        }
    }

    static class SameErasure {
        void accept(List<String> values) {
        }

        void accept(List<Integer> values) { // error: compiler.err.name.clash.same.erasure
        }
    }

    static class SameConstructor {
        SameConstructor(String name) {
        }

        SameConstructor(String other) { // error: compiler.err.already.defined
        }
    }

    static class ArrayAndVarargs {
        void take(int[] values) {
        }

        void take(int... values) { // error: compiler.err.array.and.varargs
        }
    }

    static class SameField {
        int count;
        String count; // error: compiler.err.already.defined
    }

    static class SameNestedTypeName {
        class Inner {
        }

        interface Inner { // error: compiler.err.already.defined
        }
    }

    void sameLocal() {
        int value = 1;
        int value = 2; // error: compiler.err.already.defined
    }

    void lambdaParameterNamedLikeALocal() {
        String text = "x";
        Function<String, String> identity = text -> text; // error: compiler.err.already.defined
    }
}
