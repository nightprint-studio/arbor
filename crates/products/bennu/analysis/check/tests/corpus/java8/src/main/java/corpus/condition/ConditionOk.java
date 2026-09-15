package corpus.condition;

/** Legal twins of {@link ConditionBad}. */
public class ConditionOk {
    void booleanIfCondition(boolean flag) {
        if (flag) {
            System.gc();
        }
    }

    void boxedBooleanWhileCondition(Boolean flag) {
        while (flag) {
            break;
        }
    }

    void comparison(int value) {
        if (value == 1) {
            System.gc();
        }
    }

    void booleanTernaryCondition(boolean flag) {
        int result = flag ? 1 : 2;
    }

    void assignmentOfABoolean(boolean flag) {
        boolean copy;
        if (copy = flag) {
            System.gc();
        }
    }

    void methodReturningBoolean(String text) {
        if (text.isEmpty()) {
            System.gc();
        }
    }

    void booleanForCondition() {
        for (int i = 0; i < 3; i++) {
            System.gc();
        }
    }

    void bitwiseOperatorsOnBooleans(boolean left, boolean right) {
        if (left & right | left ^ right) {
            System.gc();
        }
    }

    void notOnABoolean(boolean flag) {
        boolean result = !flag;
    }

    void conditionalAndOnComparisons(int left, int right) {
        boolean result = left > 0 && right > 0;
    }

    void instanceofCondition(Object value) {
        if (value instanceof String) {
            System.gc();
        }
    }
}
