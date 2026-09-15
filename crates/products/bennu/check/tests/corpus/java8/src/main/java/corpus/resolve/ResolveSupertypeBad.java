package corpus.resolve;

/** Supertypes that resolve to nothing. Twin: {@link ResolveSupertypeOk}. */
public class ResolveSupertypeBad extends MissingBase { // error: compiler.err.cant.resolve
    static class NestedChild extends MissingNestedBase { // error: compiler.err.cant.resolve.location
    }
}

class ResolveSupertypeBadInterface implements MissingInterface { // error: compiler.err.cant.resolve
}
