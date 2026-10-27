package corpus.decl;

import java.lang.annotation.ElementType;
import java.lang.annotation.Target;

/** More declarations the language refuses: annotations, functional interfaces, generic overrides, local types. Twin: {@link DeclMoreOk}. */
public class DeclMoreBad {
    enum Color { RED, GREEN }

    static {
        int shared = 1;
        int shared = 2; // error: compiler.err.already.defined.in.clinit
    }

    @FunctionalInterface // error: compiler.err.bad.functional.intf.anno.1
    interface TwoMethods {
        void first();
        void second();
    }

    @FunctionalInterface // error: compiler.err.bad.functional.intf.anno.1
    static class NotAnInterface { }

    @String // error: compiler.err.prob.found.req
    static class NamedAfterAClass { }

    @Target({ElementType.TYPE, ElementType.TYPE}) // error: compiler.err.repeated.annotation.target
    @interface Twice { }

    static class GenericBase<T> {
        void accept(T value) { }
    }

    static class Narrowed extends GenericBase<String> {
        void accept(Object value) { } // error: compiler.err.name.clash.same.erasure.no.override
    }

    int qualifiedLabel(Color color) {
        switch (color) {
            case Color.RED: // error: compiler.err.enum.label.must.be.unqualified.enum
                return 1;
            default:
                return 0;
        }
    }

    void localInterface() {
        interface Local { } // error: compiler.err.intf.not.allowed.here
    }

    void localAnnotation() {
        @interface LocalAnnotation { } // error: compiler.err.annotation.decl.not.allowed.here
    }
}
