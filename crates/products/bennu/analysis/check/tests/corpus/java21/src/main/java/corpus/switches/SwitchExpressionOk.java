package corpus.switches;

/** Legal twins of {@link SwitchExpressionBad} and {@link SwitchYieldBad}. */
public class SwitchExpressionOk {
    int yieldInAColonFormSwitchExpression(int key) {
        return switch (key) { case 1: yield 10; default: yield 0; };
    }

    int blockArmYields(int key) {
        return switch (key) { case 1 -> { int doubled = key * 2; yield doubled; } default -> 0; };
    }

    int blockArmYieldsOnEveryBranch(int key, boolean flag) {
        return switch (key) { case 1 -> { if (flag) { yield 1; } yield 2; } default -> 0; };
    }

    int throwingArmBesideAValueArm(int key) {
        return switch (key) { case 1 -> 1; default -> throw new IllegalStateException(); };
    }

    int breakOfALoopInsideAnArm(int key) {
        return switch (key) { case 1 -> { for (int i = 0; i < 3; i++) { break; } yield 1; } default -> 0; };
    }

    void armsOfCompatibleTypes(int key) {
        double result = switch (key) { case 1 -> 1; default -> 2.5; };
    }

    String nestedSwitchExpressions(int outer, int inner) {
        return switch (outer) { case 1 -> switch (inner) { case 1 -> "a"; default -> "b"; }; default -> "c"; };
    }

    void statementArmsInASwitchStatement(int key) {
        switch (key) { case 1 -> System.gc(); default -> { } }
    }
}
