package corpus.records;

/** Legal twins of {@link RecordCtorBad} and {@link RecordFlowBad}. */
public class RecordCtorOk {
    record CompactValidates(int x) {
        CompactValidates {
            if (x < 0) {
                throw new IllegalArgumentException();
            }
        }
    }

    record CompactReassignsItsParameter(String name) {
        CompactReassignsItsParameter {
            name = name.trim();
        }
    }

    record ExplicitCanonicalAssignsEveryField(int x, int y) {
        ExplicitCanonicalAssignsEveryField(int x, int y) {
            this.x = x;
            this.y = y;
        }
    }

    record NonCanonicalDelegates(int x, int y) {
        NonCanonicalDelegates(int x) {
            this(x, 0);
        }
    }

    public record PublicCanonical(int x) {
        public PublicCanonical(int x) {
            this.x = x;
        }
    }

    record CompactCapturesAComponent(int x) {
        CompactCapturesAComponent {
            Runnable task = () -> System.out.println(x);
        }
    }
}
