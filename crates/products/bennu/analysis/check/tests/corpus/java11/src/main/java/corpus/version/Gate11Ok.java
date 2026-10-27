package corpus.version;

import java.io.IOException;
import java.io.StringReader;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BiFunction;

/** Everything Java 11 already allows, next to the features of the later levels it must not be mistaken for. */
public class Gate11Ok {
    interface Greeter {
        String name();

        default String greet() {
            return prefix() + name(); // a `default` body needs 8
        }

        private String prefix() { // `private` needs 9
            return "hello ";
        }
    }

    void localVariableInference() {
        var names = new ArrayList<String>(); // `var` needs 10
        for (var name : names) {
            System.out.println(name);
        }
        for (var i = 0; i < 3; i++) {
            System.out.println(i);
        }
    }

    void varInLambdaParameters() {
        BiFunction<Integer, Integer, Integer> sum = (var left, var right) -> left + right; // needs 11
        System.out.println(sum.apply(1, 2));
    }

    void tryWithResourcesOnAnEffectivelyFinalVariable() throws IOException {
        StringReader reader = new StringReader("a"); // needs 9
        try (reader) {
            reader.read();
        }
    }

    void diamondWithAnAnonymousClass() {
        List<String> names = new ArrayList<>() { // needs 9
        };
        System.out.println(names);
    }

    void stringSwitchAndMultiCatch(String key) {
        switch (key) {
            case "a":
            case "b":
                System.gc();
                break;
            default:
                break;
        }
        try {
            System.gc();
        } catch (IllegalStateException | IllegalArgumentException failure) {
            throw failure;
        }
    }

    void newStringApisOfJava11(String text) {
        System.out.println(text.isBlank() + text.strip() + text.repeat(2));
    }

    int yieldIsJustAnIdentifierBeforeJava14(int value) {
        int yield = value; // a variable named like the contextual keyword
        return yield;
    }

    boolean aRecordNamedMethodIsFine(Object value) {
        return value instanceof String; // no binding variable: legal everywhere
    }

    void methodsNamedLikeTheNewKeywords() {
        record(1);
        sealed();
    }

    void record(int value) {
        System.out.println(value);
    }

    void sealed() {
        System.gc();
    }
}
