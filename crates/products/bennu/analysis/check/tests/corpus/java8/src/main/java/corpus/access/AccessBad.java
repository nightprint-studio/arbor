package corpus.access;

import corpus.access.lib.Shown;

/** Members and types seen from another package. Twin: {@link AccessOk}. */
public class AccessBad {
    int fromPackageField(Shown shown) {
        return shown.packageField; // error: compiler.err.not.def.public.cant.access
    }

    void fromPackageMethod(Shown shown) {
        shown.packageMethod(); // error: compiler.err.not.def.public.cant.access
    }

    void fromProtected(Shown shown) {
        shown.protectedMethod(); // error: compiler.err.report.access
    }

    int fromPrivate(Shown shown) {
        return shown.secret; // error: compiler.err.report.access
    }

    Object hiddenType() {
        return new corpus.access.lib.Hidden(); // error: compiler.err.not.def.public.cant.access
    }
}
