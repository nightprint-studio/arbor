package corpus.condition;

/** Conditions and logical operands that are not boolean. Twin: {@link ConditionOk}. */
public class ConditionBad {
    void intIfCondition(int value) {
        if (value) { // error: compiler.err.prob.found.req
            System.gc();
        }
    }

    void stringWhileCondition(String text) {
        while (text) { // error: compiler.err.prob.found.req
            System.gc();
        }
    }

    void intTernaryCondition(int value) {
        int result = value ? 1 : 2; // error: compiler.err.prob.found.req
    }

    void assignmentInsteadOfAComparison(int value) {
        if (value = 1) { // error: compiler.err.prob.found.req
            System.gc();
        }
    }

    void intForCondition() {
        for (int i = 0; 1; i++) { // error: compiler.err.prob.found.req
            System.gc();
        }
    }

    void boxedIntegerDoWhileCondition(Integer value) {
        do {
            System.gc();
        } while (value); // error: compiler.err.prob.found.req
    }

    void nullCondition() {
        if (null) { // error: compiler.err.prob.found.req
            System.gc();
        }
    }

    void notOnAnInt(int value) {
        boolean result = !value; // error: compiler.err.operator.cant.be.applied
    }

    void conditionalAndOnInts(int left, int right) {
        boolean result = left && right; // error: compiler.err.operator.cant.be.applied.1
    }
}
