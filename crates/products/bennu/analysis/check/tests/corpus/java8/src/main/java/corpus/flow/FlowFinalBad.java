package corpus.flow;

/** Assignments to final variables, fields and parameters. Twin: {@link FlowFinalOk}. */
public class FlowFinalBad {
    private final int field = 1;
    private static final int CONSTANT = 1;

    void finalLocal() {
        final int value = 1;
        value = 2; // error: compiler.err.cant.assign.val.to.var
    }

    void finalLocalIncremented() {
        final int value = 1;
        value++; // error: compiler.err.cant.assign.val.to.var
    }

    void finalLocalCompoundAssignment() {
        final int value = 1;
        value += 2; // error: compiler.err.cant.assign.val.to.var
    }

    void finalField() {
        field = 2; // error: compiler.err.cant.assign.val.to.var
    }

    void finalFieldThroughThis() {
        this.field = 2; // error: compiler.err.cant.assign.val.to.var
    }

    void staticFinalField() {
        CONSTANT = 2; // error: compiler.err.cant.assign.val.to.var
    }

    void finalParameter(final int value) {
        value = 2; // error: compiler.err.final.parameter.may.not.be.assigned
    }

    void finalEnhancedForVariable(int[] values) {
        for (final int value : values) {
            value = 0; // error: compiler.err.var.might.already.be.assigned
        }
    }

    void finalCatchParameter() {
        try {
            System.gc();
        } catch (final RuntimeException ex) {
            ex = null; // error: compiler.err.final.parameter.may.not.be.assigned
        }
    }

    void multiCatchParameterIsImplicitlyFinal() {
        try {
            System.gc();
        } catch (IllegalStateException | IllegalArgumentException ex) {
            ex = null; // error: compiler.err.multicatch.parameter.may.not.be.assigned
        }
    }

    void jdkConstant() {
        Math.PI = 3; // error: compiler.err.cant.assign.val.to.var
    }

    void arrayLength(int[] values) {
        values.length = 3; // error: compiler.err.cant.assign.val.to.var
    }
}
