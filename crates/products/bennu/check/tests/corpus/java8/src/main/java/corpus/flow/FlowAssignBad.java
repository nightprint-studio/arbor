package corpus.flow;

/**
 * Definite-assignment errors: finals never initialized, locals read before they are assigned.
 * Constructors are kept on one line because javac reports a missed field at the constructor's
 * closing brace. Twin: {@link FlowAssignOk}.
 */
public class FlowAssignBad {
    static class FinalFieldWithoutAnyConstructor {
        private final int value; // error: compiler.err.var.not.initialized.in.default.constructor
    }

    static class ConstructorMissesOneFinalField {
        private final int first;
        private final int second;

        ConstructorMissesOneFinalField() { first = 1; } // error: compiler.err.var.might.not.have.been.initialized
    }

    static class OneOfTwoConstructorsMissesIt {
        private final int value;

        OneOfTwoConstructorsMissesIt() { value = 1; }

        OneOfTwoConstructorsMissesIt(String name) { } // error: compiler.err.var.might.not.have.been.initialized
    }

    static class StaticFinalNeverInitialized {
        static final int VALUE; // error: compiler.err.var.might.not.have.been.initialized
    }

    void localReadBeforeAssignment() {
        int value;
        System.out.println(value); // error: compiler.err.var.might.not.have.been.initialized
    }

    void localAssignedOnOneBranchOnly(boolean flag) {
        int value;
        if (flag) {
            value = 1;
        }
        System.out.println(value); // error: compiler.err.var.might.not.have.been.initialized
    }

    void localAssignedOnlyInsideTheTry() {
        int value;
        try {
            value = Integer.parseInt("1");
        } catch (NumberFormatException ex) {
            System.gc();
        }
        System.out.println(value); // error: compiler.err.var.might.not.have.been.initialized
    }

    void blankFinalAssignedTwice() {
        final int value;
        value = 1;
        value = 2; // error: compiler.err.var.might.already.be.assigned
    }

    void blankFinalAssignedInALoop() {
        final int value;
        while (Math.random() > 0.5) {
            value = 1; // error: compiler.err.var.might.be.assigned.in.loop
        }
    }
}
