package corpus.decl;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** Declarations and creations the language refuses. Twin: {@link DeclOk}. */
public class DeclBad {
    static abstract class Shape { }

    enum Color { RED }

    Shape shape = new Shape(); // error: compiler.err.abstract.cant.be.instantiated

    Color color = new Color(); // error: compiler.err.enum.cant.be.instantiated

    static class FakeEnum extends Enum<FakeEnum> { } // error: compiler.err.enum.no.subclassing

    static class ExtendsEnum extends Color { } // error: compiler.err.cant.inherit.from.final compiler.err.enum.types.not.extensible

    enum Finalizing {
        ONE;
        protected void finalize() { } // error: compiler.err.enum.no.finalize
    }

    enum StaticTouch {
        ONE;
        static int counter;
        StaticTouch() { counter++; } // error: compiler.err.illegal.enum.static.ref
    }

    native void nativeWithBody() { } // error: compiler.err.native.meth.cant.have.body

    static class Twice implements Runnable, Runnable { // error: compiler.err.repeated.interface
        public void run() { }
    }

    class Inner {
        static void staticInInner() { } // error: compiler.err.icls.cant.have.static.decl
    }

    class HoldsInterface {
        interface Nested { } // error: compiler.err.icls.cant.have.static.decl
    }

    List<String> diamondFails = new ArrayList<>("a", "b"); // error: compiler.err.cant.apply.diamond

    void explicitArgsMissing() {
        this.<String>missing(1); // error: compiler.err.cant.resolve.args.params
    }

    Object viaInstance(Comparator<String> comparator) {
        return comparator.naturalOrder(); // error: compiler.err.illegal.static.intf.meth.call
    }

    int forward = later; // error: compiler.err.illegal.forward.ref
    int later = 1;
}

class Vault {
    private int secret;
    private void hidden() { }
}

class Intruder {
    int peek(Vault vault) {
        vault.hidden(); // error: compiler.err.report.access
        return vault.secret; // error: compiler.err.report.access
    }
}

class Duplicate { }

class Duplicate { } // error: compiler.err.duplicate.class
