package corpus.resolve;

import corpus.imports.lib.ImportLib;

/** The ways a class of another package IS in scope, plus the ones that need no import at all. Twin: {@link ResolveScopeBad}. */
public class ResolveScopeOk {
    static class Inner {
        static final int VALUE = 1;
    }

    int singleImport() {
        return ImportLib.LIMIT + ImportLib.twice(2);
    }

    int fullyQualified() {
        return corpus.imports.lib.LibWidgets.VALUE;
    }

    int declaredInThisFile() {
        return Inner.VALUE;
    }

    int javaLang() {
        return Integer.MAX_VALUE + Math.abs(-1);
    }

    int sameFilePackage() {
        return ResolveImportOk.class.hashCode();
    }
}
