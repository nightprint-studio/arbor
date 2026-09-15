package corpus.flow;

/** Legal twins of {@link FlowAssignBad}, plus flow shapes a naive definite-assignment check gets wrong. */
public class FlowAssignOk {
    static class FinalFieldWithAnInitializer {
        private final int value = 1;
    }

    static class FinalFieldAssignedInEveryConstructor {
        private final int value;

        FinalFieldAssignedInEveryConstructor() { value = 1; }

        FinalFieldAssignedInEveryConstructor(String name) { this(); }
    }

    static class FinalFieldAssignedInAnInitializerBlock {
        private final int value;

        {
            value = 1;
        }
    }

    static class StaticFinalAssignedInAStaticBlock {
        static final int VALUE;

        static {
            VALUE = 1;
        }
    }

    void localAssignedBeforeItIsRead() {
        int value;
        value = 1;
        System.out.println(value);
    }

    void localAssignedOnBothBranches(boolean flag) {
        int value;
        if (flag) {
            value = 1;
        } else {
            value = 2;
        }
        System.out.println(value);
    }

    void otherBranchThrows(boolean flag) {
        int value;
        if (flag) {
            value = 1;
        } else {
            throw new IllegalStateException();
        }
        System.out.println(value);
    }

    void assignedOnEverySwitchPath(int key) {
        int value;
        switch (key) {
            case 1:
                value = 1;
                break;
            default:
                value = 2;
        }
        System.out.println(value);
    }

    void assignedBeforeBreakingAnInfiniteLoop() {
        int value;
        while (true) {
            value = 1;
            break;
        }
        System.out.println(value);
    }

    void catchRethrows() {
        int value;
        try {
            value = Integer.parseInt("1");
        } catch (NumberFormatException ex) {
            throw ex;
        }
        System.out.println(value);
    }

    void constantTrueCondition() {
        int value;
        if (true) {
            value = 1;
        }
        System.out.println(value);
    }

    void assignedInsideAConditionalAnd(boolean flag) {
        int value;
        if (flag && (value = 1) > 0) {
            System.out.println(value);
        }
    }
}
