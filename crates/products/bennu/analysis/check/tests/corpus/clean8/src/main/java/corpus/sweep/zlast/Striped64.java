package corpus.sweep.zlast;

/** An unrelated type that is also called {@code Cell}: the project-wide simple-name map keeps only one of the two. */
abstract class Striped64 {
    static final class Cell {
        volatile long value;
    }

    Cell[] cells;
}
