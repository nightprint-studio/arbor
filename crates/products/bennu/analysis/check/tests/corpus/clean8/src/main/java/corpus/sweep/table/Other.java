package corpus.sweep.table;

/** Another nested {@code Entries}, abstract, in an unrelated outer type. */
abstract class Other {
    abstract static class Entries {
        abstract int count();
    }

    static class Concrete {
        final class Entries {
            int count() {
                return 1;
            }
        }

        Object make() {
            return new Entries();
        }
    }
}
