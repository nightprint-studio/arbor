package corpus.modern;

/** Java 10-17 features used wrongly: `var`, sealed hierarchies, anonymous diamonds, patterns that test nothing. Twin: {@link Modern17Ok}. */
public class Modern17Bad {
    sealed interface Shape permits Circle { }

    static final class Circle implements Shape { }

    static abstract class Base<T> { }

    void vars() {
        var none; // error: compiler.err.cant.infer.local.var.type
        var nothing = null; // error: compiler.err.cant.infer.local.var.type
        var array = {1, 2}; // error: compiler.err.cant.infer.local.var.type
        var lambda = () -> 1; // error: compiler.err.cant.infer.local.var.type
        var reference = String::length; // error: compiler.err.cant.infer.local.var.type
    }

    var field = 1; // error: compiler.err.restricted.type.not.allowed.here

    void localHierarchy() {
        class Local implements Shape { } // error: compiler.err.local.classes.cant.extend.sealed
        sealed class Closed { } // error: compiler.err.sealed.or.non.sealed.local.classes.not.allowed
    }

    Base<String> anonymousDiamond() {
        return new Base<>() {
            @Override // error: compiler.err.method.does.not.override.superclass
            void notInTheSupertype() { }
        };
    }

    boolean patternTestsNothing(String text) {
        return text instanceof String same; // error: compiler.err.feature.not.supported.in.source.plural
    }

    void resourceReassigned() throws Exception {
        AutoCloseable resource = () -> { };
        resource = null;
        try (resource) { } // error: compiler.err.try.with.resources.expr.effectively.final.var
    }
}
