package corpus.records;

/** A canonical constructor that leaves a component field unassigned (a flow error, kept apart from attribution errors). Twin: {@link RecordCtorOk}. */
public class RecordFlowBad {
    record CanonicalLeavesAFieldUnset(int x, int y) {
        CanonicalLeavesAFieldUnset(int x, int y) { this.x = x; } // error: compiler.err.var.might.not.have.been.initialized
    }
}
