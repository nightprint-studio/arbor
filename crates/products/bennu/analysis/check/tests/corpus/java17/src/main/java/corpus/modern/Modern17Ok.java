package corpus.modern;

import java.util.ArrayList;
import java.util.function.Supplier;

/** The legal shapes of what {@link Modern17Bad} gets wrong. */
public class Modern17Ok {
    sealed interface Shape permits Circle { }

    static final class Circle implements Shape { }

    static abstract class Base<T> {
        abstract T make();
    }

    void vars() {
        var number = 1;
        var list = new ArrayList<String>();
        Supplier<Integer> supplier = () -> 1;
        var fromSupplier = supplier.get();
        for (var item : list) {
            var length = item.length();
        }
    }

    void localClasses() {
        class Local { }
        new Local();
    }

    Base<String> anonymousDiamond() {
        return new Base<>() {
            @Override
            String make() { return ""; }
        };
    }

    boolean patternTestsSomething(Object value) {
        return value instanceof String text && text.isEmpty();
    }

    void resourceEffectivelyFinal() throws Exception {
        AutoCloseable resource = () -> { };
        try (resource) { }
    }
}
