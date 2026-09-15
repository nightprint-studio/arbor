package corpus.flow;

/** Legal twins of {@link FlowFinalBad}. */
public class FlowFinalOk {
    private static final int CONSTANT;
    private final int assignedInTheConstructor;
    private int mutable;

    static {
        CONSTANT = 1;
    }

    FlowFinalOk() {
        assignedInTheConstructor = 1;
    }

    void mutableLocal() {
        int value = 1;
        value = 2;
        value++;
        value += 2;
    }

    void mutableField() {
        mutable = 2;
        this.mutable++;
    }

    void blankFinalLocalAssignedOnce() {
        final int value;
        value = 1;
    }

    void blankFinalAssignedOnEachBranch(boolean flag) {
        final int value;
        if (flag) {
            value = 1;
        } else {
            value = 2;
        }
    }

    void mutableParameter(int value) {
        value = 2;
    }

    void finalArrayContentsAreMutable() {
        final int[] values = { 1 };
        values[0] = 2;
    }

    void finalReferenceStateIsMutable() {
        final StringBuilder builder = new StringBuilder();
        builder.append("x");
    }

    void enhancedForVariable(int[] values) {
        for (int value : values) {
            value = 0;
        }
    }

    void catchParameter() {
        try {
            System.gc();
        } catch (RuntimeException ex) {
            ex = null;
        }
    }
}
