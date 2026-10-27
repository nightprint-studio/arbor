package corpus.inherit;

/** Supertype cycles, each pair on one line so the reported line does not depend on which type javac picks. Twin: {@link InheritCycleOk}. */
public class InheritCycleBad {
    static class Alpha extends Beta { } static class Beta extends Alpha { } // error: compiler.err.cyclic.inheritance

    static class Self extends Self { } // error: compiler.err.cyclic.inheritance

    interface First extends Second { } interface Second extends First { } // error: compiler.err.cyclic.inheritance
}
