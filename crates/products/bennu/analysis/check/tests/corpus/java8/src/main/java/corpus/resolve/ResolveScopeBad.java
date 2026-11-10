package corpus.resolve;

/** A class of another package used without an import: javac cannot find the symbol, though the class exists in the project. Twin: {@link ResolveScopeOk}. */
public class ResolveScopeBad {
    int staticField() {
        return ImportLib.LIMIT; // error: compiler.err.cant.resolve.location
    }

    int staticCall() {
        return ImportLib.twice(2); // error: compiler.err.cant.resolve.location
    }
}
