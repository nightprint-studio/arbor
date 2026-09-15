package corpus.modern;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.BiFunction;

/** Legal twins of {@link ModernBad}, plus language-21 scoping and typing a naive checker gets wrong. */
public class ModernOk {
    void takesInt(int value) {
    }

    void takesString(String value) {
    }

    void takesDouble(double value) {
    }

    void recordConstructorAndMembers() {
        ModernTypes.Point point = new ModernTypes.Point(1, 2);
        takesInt(point.x());
        takesInt(point.sum());
        ModernTypes.Point origin = ModernTypes.Point.origin();
    }

    void varLocal() {
        var number = 1;
        takesInt(number);
    }

    void patternVariable(Object value) {
        if (value instanceof String text) {
            takesString(text);
        }
    }

    void patternVariableInScopeAfterANegatedTest(Object value) {
        if (!(value instanceof String text)) {
            return;
        }
        takesString(text);
    }

    void patternVariableInAConjunction(Object value) {
        if (value instanceof String text && text.length() > 2) {
            takesString(text);
        }
    }

    void recordPatternComponents(Object value) {
        if (value instanceof ModernTypes.Point(int x, int y)) {
            takesInt(x + y);
        }
    }

    void switchExpressionArgument(ModernTypes.Mode mode) {
        takesString(switch (mode) { case FAST -> "a"; case SLOW -> "b"; });
    }

    void switchExpressionWithYield(int key) {
        int result = switch (key) {
            case 1 -> 10;
            default -> {
                int doubled = key * 2;
                yield doubled;
            }
        };
    }

    void guardedPatternSwitch(Object value) {
        int result = switch (value) {
            case Integer number when number > 0 -> number;
            case Integer number -> -number;
            default -> 0;
        };
    }

    void textBlockArgument() {
        takesString(
            """
            text
            """);
    }

    void patternSwitchArms(ModernTypes.Shape shape) {
        switch (shape) {
            case ModernTypes.Circle circle -> takesDouble(circle.radius());
            case ModernTypes.Square square -> takesDouble(square.side());
        }
    }

    void exhaustiveSealedSwitchExpression(ModernTypes.Shape shape) {
        double area = switch (shape) {
            case ModernTypes.Circle circle -> circle.radius() * circle.radius();
            case ModernTypes.Square square -> square.side() * square.side();
        };
    }

    void varElement() {
        var names = List.of("a");
        takesString(names.get(0));
    }

    void varLambdaParameters() {
        BiFunction<Integer, Integer, Integer> add = (var a, var b) -> a + b;
    }

    void modernJdkApis() {
        "abc".repeat(2);
        boolean empty = Optional.of("x").isEmpty();
        List<Integer> collected = List.of(1).stream().toList();
    }

    void sequencedCollections() {
        takesString(List.of("a").getFirst());
        new ArrayList<String>().addFirst("x");
    }

    void varForEach(Map<String, Integer> counts) {
        for (var entry : counts.entrySet()) {
            takesString(entry.getKey());
        }
    }

    void varHoldingAnAnonymousClass() {
        var anonymous = new Object() {
            int hidden() {
                return 1;
            }
        };
        takesInt(anonymous.hidden());
    }

    void localRecord() {
        record Local(int amount) {
        }
        takesInt(new Local(1).amount());
    }

    void genericTypePattern(Object value) {
        if (value instanceof List<?> list) {
            takesInt(list.size());
        }
    }
}
