package corpus.flow;

import java.util.function.IntSupplier;

/** Legal twins of {@link FlowReturnBad}, plus bodies that cannot complete normally and so need no return. */
public class FlowReturnOk {
    int returnsOnEveryBranch(boolean flag) { if (flag) { return 1; } else { return 2; } }

    int returnsAfterTheBranch(boolean flag) { if (flag) { return 1; } return 2; }

    int endsWithAThrow() { throw new UnsupportedOperationException(); }

    int infiniteWhileLoop() { while (true) { System.gc(); } }

    int infiniteForLoop() { for (;;) { System.gc(); } }

    int switchWithADefault(int key) { switch (key) { case 1: return 1; default: return 0; } }

    String tryAndCatchBothReturn() { try { return String.valueOf(1); } catch (RuntimeException ex) { return ""; } }

    void voidMethodNeedsNoReturn(boolean flag) { if (flag) { return; } }

    IntSupplier lambdaReturnsOnEveryPath(boolean flag) { return () -> { if (flag) { return 1; } return 2; }; }

    int labeledInfiniteLoopWithContinue() { outer: while (true) { for (int i = 0; i < 3; i++) { continue outer; } } }
}
