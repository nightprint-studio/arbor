package corpus.switches;

/** Switch-expression arms that complete without yielding a value (flow errors). One line per switch. Twin: {@link SwitchExpressionOk}. */
public class SwitchYieldBad {
    int blockArmWithoutAYield(int key) {
        return switch (key) { case 1 -> { System.gc(); } default -> 0; }; // error: compiler.err.rule.completes.normally
    }

    int blockArmYieldingOnOneBranchOnly(int key, boolean flag) {
        return switch (key) { case 1 -> { if (flag) { yield 1; } } default -> 0; }; // error: compiler.err.rule.completes.normally
    }

    int colonFormFallsOutOfTheLastCase(int key) {
        return switch (key) { case 1: yield 1; default: System.gc(); }; // error: compiler.err.switch.expression.completes.normally
    }
}
