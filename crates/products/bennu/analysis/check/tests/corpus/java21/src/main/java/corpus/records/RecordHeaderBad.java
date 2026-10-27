package corpus.records;

/** Components a record header may not declare. Twin: {@link RecordShapesOk}. */
public class RecordHeaderBad {
    record HashCodeComponent(int hashCode) { // error: compiler.err.illegal.record.component.name
    }

    record ToStringComponent(String toString) { // error: compiler.err.illegal.record.component.name
    }

    record GetClassComponent(Object getClass) { // error: compiler.err.illegal.record.component.name
    }
}
