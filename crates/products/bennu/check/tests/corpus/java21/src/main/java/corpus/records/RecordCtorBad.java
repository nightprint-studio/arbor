package corpus.records;

/** Record constructors that break the canonical-constructor rules. Twin: {@link RecordCtorOk}. */
public class RecordCtorBad {
    record NonCanonicalNotDelegating(int x, int y) {
        NonCanonicalNotDelegating(int x) { // error: compiler.err.first.statement.must.be.call.to.another.constructor
        } // error: compiler.err.var.might.not.have.been.initialized
    }

    record CanonicalWithRenamedParameters(int x) {
        CanonicalWithRenamedParameters(int other) { // error: compiler.err.invalid.canonical.constructor.in.record
            this.x = other;
        }
    }

    public record CanonicalLessAccessibleThanTheRecord(int x) {
        CanonicalLessAccessibleThanTheRecord(int x) { // error: compiler.err.invalid.canonical.constructor.in.record
            this.x = x;
        }
    }

    record CanonicalWithAThrowsClause(int x) {
        CanonicalWithAThrowsClause(int x) throws Exception { // error: compiler.err.invalid.canonical.constructor.in.record
            this.x = x;
        }
    }

    record CompactAssigningTheField(int x) {
        CompactAssigningTheField {
            this.x = 1; // error: compiler.err.cant.assign.val.to.var
        }
    }

    record CompactWithAReturn(int x) {
        CompactWithAReturn { // error: compiler.err.invalid.canonical.constructor.in.record
            return; // error: compiler.err.var.might.not.have.been.initialized
        }
    }
}
