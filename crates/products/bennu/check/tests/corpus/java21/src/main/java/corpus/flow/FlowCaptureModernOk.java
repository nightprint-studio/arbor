package corpus.flow;

import java.util.List;
import java.util.function.Supplier;

/** Legal twins of {@link FlowCaptureModernBad}. */
public class FlowCaptureModernOk {
    void effectivelyFinalVar() {
        var count = 0;
        Supplier<Integer> supplier = () -> count;
    }

    void patternVariableNotReassigned(Object value) {
        if (value instanceof String text) {
            Runnable task = () -> System.out.println(text);
        }
    }

    void varCapturedByAnAnonymousClass() {
        var count = 0;
        Runnable task = new Runnable() { public void run() { System.out.println(count); } };
    }

    void capturedInsideASwitchExpressionArm(int key) {
        var total = key * 2;
        Supplier<Integer> supplier = switch (key) { case 1 -> () -> total; default -> () -> 0; };
    }

    void varInAnEnhancedFor(List<String> names) {
        for (var name : names) {
            Runnable task = () -> System.out.println(name);
        }
    }

    void textBlockLocal() {
        var text = """
            captured
            """;
        Runnable task = () -> System.out.println(text);
    }
}
