package corpus.sweep.table;

/** Two levels below {@code Table}: the inherited {@code Cell} is neither imported nor declared in this file. */
abstract class RegularTable<R, V> extends AbstractTable<R, V> {
    abstract Cell<R, V> getCell(int index);

    Cell<R, V> first() {
        return getCell(0);
    }
}
