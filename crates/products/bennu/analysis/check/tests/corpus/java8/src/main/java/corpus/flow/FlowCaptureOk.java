package corpus.flow;

import java.util.List;
import java.util.function.IntSupplier;

/** Legal twins of {@link FlowCaptureBad}: captures of effectively final locals, and mutations that are not captures. */
public class FlowCaptureOk {
    private int counter;

    void effectivelyFinalLocal() {
        int count = 0;
        IntSupplier supplier = () -> count;
    }

    void explicitlyFinalLocal() {
        final int count = 0;
        IntSupplier supplier = () -> count;
    }

    void assignedExactlyOnceAfterItsDeclaration() {
        int count;
        count = 1;
        IntSupplier supplier = () -> count;
    }

    void effectivelyFinalParameter(String name) {
        Runnable task = () -> System.out.println(name);
    }

    void enhancedForVariable(List<String> names) {
        for (String name : names) {
            Runnable task = () -> System.out.println(name);
        }
    }

    void copyOfTheLoopVariable() {
        for (int i = 0; i < 3; i++) {
            int copy = i;
            Runnable task = () -> System.out.println(copy);
        }
    }

    void fieldMutatedInsideTheLambda() {
        Runnable task = () -> counter++;
    }

    void arrayElementMutatedInsideTheLambda() {
        int[] box = { 0 };
        Runnable task = () -> box[0]++;
    }

    void lambdaLocalReassignedInsideTheLambda() {
        Runnable task = () -> {
            int local = 0;
            local++;
        };
    }

    void anonymousClassCapturesAnEffectivelyFinalLocal() {
        int count = 0;
        Runnable task = new Runnable() { public void run() { System.out.println(count); } };
    }

    void localClassCapturesAnEffectivelyFinalLocal() {
        int count = 0;
        class Local { int read() { return count; } }
    }
}
