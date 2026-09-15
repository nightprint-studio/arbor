package corpus.flow;

import java.util.function.Supplier;

/** Non-effectively-final captures through language-21 constructs. Twin: {@link FlowCaptureModernOk}. */
public class FlowCaptureModernBad {
    void varReassignedBeforeTheLambda() {
        var count = 0;
        count++;
        Supplier<Integer> supplier = () -> count; // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void reassignedPatternVariable(Object value) {
        if (value instanceof String text) {
            text = text.trim();
            Runnable task = () -> System.out.println(text); // error: compiler.err.cant.ref.non.effectively.final.var
        }
    }

    void varCapturedByAnAnonymousClass() {
        var count = 0;
        count = 1;
        Runnable task = new Runnable() { public void run() { System.out.println(count); } }; // error: compiler.err.cant.ref.non.effectively.final.var
    }

    void capturedInsideASwitchExpressionArm(int key) {
        var total = 0;
        total += key;
        Supplier<Integer> supplier = switch (key) { case 1 -> () -> total; default -> () -> 0; }; // error: compiler.err.cant.ref.non.effectively.final.var
    }
}
