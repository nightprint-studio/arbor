package corpus.resolve;

/** Names in value position that bind to nothing. Twin: {@link ResolveVariableOk}. */
public class ResolveVariableBad {
    private int counter = 1;

    static class Box {
        int size;
        static int COUNT;
    }

    void localNeverDeclared() {
        int result = undeclared + 1; // error: compiler.err.cant.resolve.location
    }

    void localOutOfScope() {
        {
            int inner = 1;
        }
        int result = inner; // error: compiler.err.cant.resolve.location
    }

    void fieldOnInstance(Box box) {
        int result = box.missing; // error: compiler.err.cant.resolve.location
    }

    void staticFieldOnProjectType() {
        int result = Box.MISSING; // error: compiler.err.cant.resolve.location
    }

    void staticFieldOnJdkType() {
        double result = Math.NOT_A_CONSTANT; // error: compiler.err.cant.resolve.location
    }

    void loopVariableAfterLoop() {
        for (int i = 0; i < 3; i++) {
            counter++;
        }
        int result = i; // error: compiler.err.cant.resolve.location
    }

    void catchParameterAfterCatch() {
        try {
            counter++;
        } catch (RuntimeException ex) {
            counter--;
        }
        Object result = ex; // error: compiler.err.cant.resolve.location
    }

    void misspelledFieldThroughThis() {
        int result = this.countr; // error: compiler.err.cant.resolve
    }

    void lambdaLocalOutsideLambda() {
        Runnable task = () -> {
            int inside = 1;
        };
        int result = inside; // error: compiler.err.cant.resolve.location
    }

    void unknownQualifier() {
        int result = Nowhere.VALUE; // error: compiler.err.cant.resolve.location
    }
}
