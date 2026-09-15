package corpus.flow;

import java.util.function.IntSupplier;

/** Locals captured by a lambda, anonymous or local class without being effectively final. Twin: {@link FlowCaptureOk}. */
public class FlowCaptureBad {
    void reassignedBeforeTheLambda() {
        int count = 0;
        count++;
        IntSupplier supplier = () -> count; // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void reassignedAfterTheLambda() {
        int count = 0;
        IntSupplier supplier = () -> count; // error: compiler.err.cant.ref.non.effectively.final.var
        count = 5;
    }

    void reassignedInsideTheLambda() {
        int count = 0;
        Runnable task = () -> count++; // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void capturedByAnAnonymousClass() {
        int count = 0;
        count = 1;
        Runnable task = new Runnable() { public void run() { System.out.println(count); } }; // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void capturedByALocalClass() {
        int count = 0;
        count = 1;
        class Local { int read() { return count; } } // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void reassignedParameter(String name) {
        name = name.trim();
        Runnable task = () -> System.out.println(name); // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void classicLoopVariable() {
        for (int i = 0; i < 3; i++) {
            Runnable task = () -> System.out.println(i); // error: compiler.err.cant.ref.non.effectively.final.var
        }
    }
}
