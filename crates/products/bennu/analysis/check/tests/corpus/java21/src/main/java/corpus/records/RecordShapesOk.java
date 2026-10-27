package corpus.records;

/** Legal record shapes: the twins of {@link RecordSuperBad}, {@link RecordHeaderBad} and {@link RecordLocalBad}. */
public class RecordShapesOk {
    interface Printable {
        default String print() {
            return "printable";
        }
    }

    record Empty() {
    }

    record Generic<T extends Comparable<T>>(T first, T second) {
        Generic {
            if (first.compareTo(second) > 0) {
                throw new IllegalArgumentException();
            }
        }

        static <T extends Comparable<T>> Generic<T> of(T first, T second) {
            return new Generic<>(first, second);
        }
    }

    record WithVarargs(String... names) {
    }

    record Ranked(int value) implements Comparable<Ranked> {
        public int compareTo(Ranked other) {
            return Integer.compare(value, other.value);
        }
    }

    record StaticMembers(int x) {
        static final int ZERO = 0;
        static int counter;

        static {
            counter = ZERO;
        }

        StaticMembers() {
            this(ZERO);
        }

        static StaticMembers origin() {
            return new StaticMembers();
        }
    }

    record OverridesTheGeneratedMembers(String name) {
        public String name() {
            return name.trim();
        }

        @Override
        public String toString() {
            return name;
        }
    }

    record DefaultFromAnInterface(int x) implements Printable {
        public String print() {
            return Printable.super.print() + x;
        }
    }

    record ExplicitCanonical(int x, int y) {
        public ExplicitCanonical(int x, int y) {
            this.x = x;
            this.y = y;
        }

        ExplicitCanonical(int x) {
            this(x, 0);
        }
    }

    record NestedRecord(Point inner) {
        record Point(int x) {
        }
    }

    class Inner {
        record RecordInAnInnerClass(int x) {
        }
    }

    int usesAStaticallyReachableMember() {
        record Local(int x) {
            int doubled() {
                return x * 2 + StaticMembers.ZERO;
            }
        }
        return new Local(2).doubled();
    }
}
