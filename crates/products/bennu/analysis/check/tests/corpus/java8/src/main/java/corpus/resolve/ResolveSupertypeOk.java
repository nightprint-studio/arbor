package corpus.resolve;

import java.io.Serializable;

/** Legal twins of {@link ResolveSupertypeBad}: JDK, imported, nested and forward-referenced supertypes. */
public class ResolveSupertypeOk extends Object implements Runnable, Serializable {
    public void run() {
    }

    static class NestedChild extends DeclaredLater {
    }

    static class DeclaredLater {
    }
}

class ResolveSupertypeOkInterface implements java.util.function.Supplier<String> {
    public String get() {
        return "";
    }
}
