package corpus.flow;

import java.util.function.IntSupplier;

/**
 * Methods that can complete without returning a value. One line each: javac reports it at the
 * closing brace of the body. Twin: {@link FlowReturnOk}.
 */
public class FlowReturnBad {
    int noReturnAtAll() { } // error: compiler.err.missing.ret.stmt

    int returnsOnOneBranchOnly(boolean flag) { if (flag) { return 1; } } // error: compiler.err.missing.ret.stmt

    int returnsOnlyInsideTheLoop(int[] values) { for (int value : values) { return value; } } // error: compiler.err.missing.ret.stmt

    int switchWithoutADefault(int key) { switch (key) { case 1: return 1; case 2: return 2; } } // error: compiler.err.missing.ret.stmt

    String catchFallsThrough() { try { return String.valueOf(1); } catch (RuntimeException ex) { System.gc(); } } // error: compiler.err.missing.ret.stmt

    int whileWithANonConstantCondition(boolean flag) { while (flag) { return 1; } } // error: compiler.err.missing.ret.stmt

    IntSupplier lambdaReturnsOnOneBranchOnly(boolean flag) { return () -> { if (flag) { return 1; } }; } // error: compiler.err.prob.found.req
}
