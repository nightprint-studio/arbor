package corpus.records;

/** A modifier on a record component is a syntax error; alone in its file so it cannot muddy another case. */
public class RecordModifierBad {
    record FinalComponent(final int x) { // error: compiler.err.record.cant.declare.field.modifiers
    }
}
