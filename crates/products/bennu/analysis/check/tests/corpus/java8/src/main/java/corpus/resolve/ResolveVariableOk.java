package corpus.resolve;

import java.util.function.IntUnaryOperator;

/** Legal twins of {@link ResolveVariableBad}, plus scoping shapes a naive resolver gets wrong. */
public class ResolveVariableOk {
    private int counter = 1;
    private int shadowed = 1;
    static int STATIC_FIELD = 2;

    static class Box {
        int size;
        static int COUNT;
    }

    void localDeclared() {
        int declared = 1;
        int result = declared + 1;
    }

    void localInScope() {
        {
            int inner = 1;
            int result = inner;
        }
    }

    void fieldOnInstance(Box box) {
        int result = box.size;
    }

    void staticFieldOnProjectType() {
        int result = Box.COUNT;
    }

    void staticFieldOnJdkType() {
        double result = Math.PI;
    }

    void loopVariableInsideLoop() {
        for (int i = 0; i < 3; i++) {
            int result = i;
        }
    }

    void catchParameterInsideCatch() {
        try {
            counter++;
        } catch (RuntimeException ex) {
            Object result = ex;
        }
    }

    void fieldThroughThis() {
        int result = this.counter;
    }

    void implicitField() {
        int result = counter;
    }

    void fieldShadowedByLocal() {
        String shadowed = "local";
        String result = shadowed;
    }

    void fieldVisibleAgainAfterShadowingBlock() {
        {
            String shadowed = "local";
        }
        int result = shadowed;
    }

    void staticFieldUnqualified() {
        int result = STATIC_FIELD;
    }

    void arrayLength(int[] values) {
        int result = values.length;
    }

    void lambdaParameterAndCapturedField() {
        IntUnaryOperator op = value -> value + counter;
    }

    void anonymousClassReadsOuterField() {
        Runnable task = new Runnable() {
            public void run() {
                int result = counter;
            }
        };
    }

    void qualifiedOuterThis() {
        Runnable task = new Runnable() {
            public void run() {
                int result = ResolveVariableOk.this.counter;
            }
        };
    }

    void localDeclaredInEarlierSwitchCase(int key) {
        switch (key) {
            case 1:
                int shared = 1;
                break;
            default:
                shared = 2;
                int result = shared;
        }
    }
}
