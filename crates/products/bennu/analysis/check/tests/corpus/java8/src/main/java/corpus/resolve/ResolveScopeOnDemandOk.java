package corpus.resolve;

import corpus.imports.lib.*;

/** A class of another package reached through an import on demand. Twin: {@link ResolveScopeBad}. */
public class ResolveScopeOnDemandOk {
    int onDemand() {
        return ImportLib.LIMIT + ImportLib.twice(2);
    }
}
