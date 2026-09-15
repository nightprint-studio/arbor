package corpus.modern;

import java.util.List;
import java.util.function.BiFunction;

/** Resolution and argument errors inside language-21 constructs. Twin: {@link ModernOk}. */
public class ModernBad {
    void takesInt(int value) {
    }

    void takesString(String value) {
    }

    void takesDouble(double value) {
    }

    void recordConstructorTooFewArguments() {
        new ModernTypes.Point(1); // error: compiler.err.cant.apply.symbol
    }

    void recordConstructorWrongType() {
        new ModernTypes.Point("1", 2); // error: compiler.err.cant.apply.symbol
    }

    void unknownRecordComponent(ModernTypes.Point point) {
        point.z(); // error: compiler.err.cant.resolve.location.args
    }

    void accessorGivenAnArgument(ModernTypes.Point point) {
        point.x(1); // error: compiler.err.cant.apply.symbol
    }

    void varLocalAsWrongArgument() {
        var text = "s";
        takesInt(text); // error: compiler.err.cant.apply.symbol
    }

    void patternVariableAsWrongArgument(Object value) {
        if (value instanceof String text) {
            takesInt(text); // error: compiler.err.cant.apply.symbol
        }
    }

    void patternVariableOutOfScope(Object value) {
        if (!(value instanceof String text)) {
            takesString(text); // error: compiler.err.cant.resolve.location
        }
    }

    void recordPatternComponentAsWrongArgument(Object value) {
        if (value instanceof ModernTypes.Point(int x, int y)) {
            takesString(x); // error: compiler.err.cant.apply.symbol
        }
    }

    void switchExpressionAsWrongArgument(ModernTypes.Mode mode) {
        takesInt(switch (mode) { case FAST -> "a"; case SLOW -> "b"; }); // error: compiler.err.cant.apply.symbol
    }

    void textBlockAsWrongArgument() {
        takesInt( // error: compiler.err.cant.apply.symbol
            """
            text
            """);
    }

    void patternSwitchArmWrongArgument(ModernTypes.Shape shape) {
        switch (shape) {
            case ModernTypes.Circle circle -> takesString(circle.radius()); // error: compiler.err.cant.apply.symbol
            case ModernTypes.Square square -> takesDouble(square.side());
        }
    }

    void varElementAsWrongArgument() {
        var numbers = List.of(1, 2);
        takesString(numbers.get(0)); // error: compiler.err.cant.apply.symbol
    }

    void varLambdaParametersWrongReturn() {
        BiFunction<Integer, Integer, Integer> add = (var a, var b) -> a + b + ""; // error: compiler.err.prob.found.req
    }

    void modernJdkApiWrongArgument() {
        "abc".repeat("2"); // error: compiler.err.cant.apply.symbol
    }

    void toListAddWrongType() {
        List.of(1).stream().toList().add("x"); // error: compiler.err.cant.apply.symbols
    }
}
