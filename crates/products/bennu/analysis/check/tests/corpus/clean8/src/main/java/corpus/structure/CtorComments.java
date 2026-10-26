package corpus.structure;

/**
 * Comments ahead of {@code this(…)} / {@code super(…)} are not statements: a constructor may explain
 * which one it delegates to without becoming a Java 25 "statements before super" file.
 */
public class CtorComments {

    private final String name;
    private final String root;

    public CtorComments() {
        // NB: uses the general constructor, see
        //     CtorComments(String name, String root)
        this("Default",
             "/Root");
    }

    public CtorComments(String name, String root) {
        this.name = name;
        this.root = root;
    }

    public static class Derived extends CtorComments {
        public Derived() {
            /* the general constructor does the work */
            super("Derived", "/Derived");
        }

        public Derived(String name) {
            // first the comment, then the call
            /* and a block one */
            super(name, "/" + name);
        }
    }

    public String describe() {
        return name + root;
    }
}
