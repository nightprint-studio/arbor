package corpus.inherit;

/** `implements` on an interface is a parse error: alone in its file so parser recovery cannot reach other cases. Twin: {@link InheritKindOk}. */
public class InheritInterfaceImplementsBad {
    interface ImplementsSomething implements Runnable { } // error: compiler.err.expected
}
