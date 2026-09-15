package corpus.switches;

/** Switch expressions misused (attribution errors; the flow ones are in {@link SwitchYieldBad}). One line per switch. Twin: {@link SwitchExpressionOk}. */
public class SwitchExpressionBad {
    void yieldOutsideASwitchExpression() {
        yield 1; // error: compiler.err.no.switch.expression
    }

    int breakOutOfASwitchExpression(int key) {
        return switch (key) { case 1: break; default: yield 0; }; // error: compiler.err.break.outside.switch.expression
    }

    void continueOutOfASwitchExpression(int[] keys) {
        for (int key : keys) {
            int result = switch (key) { case 1: continue; default: yield 0; }; // error: compiler.err.continue.outside.switch.expression
        }
    }

    int returnInsideASwitchExpression(int key) {
        return switch (key) { case 1 -> { return 1; } default -> 0; }; // error: compiler.err.return.outside.switch.expression
    }

    int noArmProducesAValue(int key) {
        return switch (key) { default -> throw new IllegalStateException(); }; // error: compiler.err.switch.expression.no.result.expressions
    }

    int emptySwitchExpression(int key) {
        return switch (key) { }; // error: compiler.err.switch.expression.empty
    }

    void armOfTheWrongType(int key) {
        int result = switch (key) { case 1 -> "one"; default -> 0; }; // error: compiler.err.prob.found.req
    }

    void valueArmsInASwitchStatement(int key) {
        switch (key) { case 1 -> 1; default -> 0; } // error: compiler.err.not.stmt
    }
}
