package corpus.records;

import java.util.List;

/** Legal twins of {@link RecordMemberBad}: everything a record may declare. */
public class RecordMemberOk {
    record WithStaticMembers(int x) {
        static int instances;

        static {
            instances = 0;
        }

        static WithStaticMembers origin() {
            return new WithStaticMembers(0);
        }
    }

    record WithAnExplicitAccessor(int x) {
        public int x() {
            return x;
        }
    }

    record WithExtraMethods(int x, int y) {
        int sum() {
            return x + y;
        }

        WithExtraMethods withX(int newX) {
            return new WithExtraMethods(newX, y);
        }
    }

    record ImplementsAnInterface(String name) implements Comparable<ImplementsAnInterface> {
        public int compareTo(ImplementsAnInterface other) {
            return name.compareTo(other.name);
        }
    }

    record Generic<T>(T value, List<T> values) {
    }

    record WithVarargs(String... names) {
    }

    record WithNestedTypes(int x) {
        enum Kind {
            A, B
        }

        record Inner(Kind kind) {
        }
    }

    void localRecord() {
        record Local(int amount) {
        }
        int total = new Local(1).amount();
    }
}
