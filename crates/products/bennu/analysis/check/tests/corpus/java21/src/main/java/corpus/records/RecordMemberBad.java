package corpus.records;

/** Members a record may not declare, or declares wrongly. Twin: {@link RecordMemberOk}. */
public class RecordMemberBad {
    record WithAnInstanceField(int x) {
        int extra; // error: compiler.err.record.cannot.declare.instance.fields
    }

    record WithAnInstanceInitializer(int x) {
        { System.gc(); } // error: compiler.err.instance.initializer.not.allowed.in.records
    }

    record AccessorWithTheWrongReturnType(int x) {
        public long x() { // error: compiler.err.invalid.accessor.method.in.record
            return x;
        }
    }

    record NonPublicAccessor(int x) {
        int x() { // error: compiler.err.invalid.accessor.method.in.record
            return x;
        }
    }

    record AssignsAComponentField(int x) {
        void reset() {
            x = 0; // error: compiler.err.cant.assign.val.to.var
        }
    }
}
