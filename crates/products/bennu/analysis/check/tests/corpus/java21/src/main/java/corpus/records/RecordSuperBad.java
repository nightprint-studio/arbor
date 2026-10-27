package corpus.records;

/** Explicit constructor invocations and supertypes a record may not have. Twin: {@link RecordShapesOk}. */
public class RecordSuperBad {
    record CompactCallsSuper(int x) {
        CompactCallsSuper { super(); } // error: compiler.err.invalid.canonical.constructor.in.record
    }

    record CanonicalCallsSuper(int x) {
        CanonicalCallsSuper(int x) { super(); this.x = x; } // error: compiler.err.invalid.canonical.constructor.in.record
    }

    abstract static class ExtendsRecord extends Record { // error: compiler.err.invalid.supertype.record
    }
}
