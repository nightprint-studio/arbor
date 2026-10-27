package corpus.sweep.table;

public final class DenseTable<R, V> extends RegularTable<R, V> {
    private final R rows[];
    private final V values[][];

    public DenseTable(R[] rows, V[][] values) {
        this.rows = rows;
        this.values = values;
    }

    @Override
    Cell<R, V> getCell(int index) {
        final R row = rows[index];
        final V value = values[index][0];
        return new Cell<R, V>() {
            public R getRow() {
                return row;
            }

            public V getValue() {
                return value;
            }
        };
    }
}
