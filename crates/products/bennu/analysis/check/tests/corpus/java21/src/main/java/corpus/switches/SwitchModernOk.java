package corpus.switches;

/** Legal Java 21 `switch` shapes next to the ones {@link SwitchGuardBad} and {@link SwitchPatternBad} reject. */
public class SwitchModernOk {
    sealed interface Shape permits Circle, Square, Group {
    }

    record Circle(double radius) implements Shape {
    }

    record Square(double side) implements Shape {
    }

    record Group(Shape first, Shape second) implements Shape {
    }

    sealed interface Result<T> permits Success, Failure {
    }

    record Success<T>(T value) implements Result<T> {
    }

    record Failure<T>(String reason) implements Result<T> {
    }

    enum Level { LOW, HIGH }

    double exhaustiveWithoutADefault(Shape shape) {
        return switch (shape) {
            case Circle circle -> circle.radius();
            case Square square -> square.side();
            case Group group -> exhaustiveWithoutADefault(group.first()) + exhaustiveWithoutADefault(group.second());
        };
    }

    double nestedRecordPatterns(Shape shape) {
        return switch (shape) {
            case Circle(double radius) -> radius;
            case Square(var side) -> side;
            case Group(Circle(var radius), Square(var side)) -> radius + side;
            case Group(var first, var second) -> 0;
        };
    }

    <T> T genericSealedHierarchy(Result<T> result, T fallback) {
        return switch (result) {
            case Success<T> success -> success.value();
            case Failure<T> failure -> fallback;
        };
    }

    String guardsThenNullAndDefault(Object value) {
        return switch (value) {
            case String text when text.isEmpty() -> "empty";
            case String text -> text;
            case null, default -> "other";
        };
    }

    int qualifiedEnumLabels(Level level) {
        return switch (level) {
            case Level.LOW -> 1;
            case Level.HIGH -> 2;
        };
    }

    int enumWithANullArm(Level level) {
        return switch (level) {
            case null -> 0;
            case LOW -> 1;
            case HIGH -> 2;
        };
    }

    int boxedSelectorMixingConstantsAndPatterns(Integer boxed) {
        return switch (boxed) {
            case 1 -> 1;
            case Integer number when number > 5 -> 5;
            case Integer number -> 0;
        };
    }

    int yieldFromTheColonForm(String key) {
        return switch (key) {
            case "a":
                yield 1;
            case "b": {
                yield 2;
            }
            default:
                yield 0;
        };
    }

    int throwingArm(Level level) {
        return switch (level) {
            case LOW -> 1;
            case HIGH -> throw new IllegalStateException();
        };
    }

    String nestedSwitchExpressions(Shape shape, Level level) {
        return switch (shape) {
            case Circle circle -> switch (level) {
                case LOW -> "small circle";
                case HIGH -> "big circle";
            };
            default -> "other";
        };
    }

    void statementOverASealedTypeIsExhaustive(Shape shape) {
        switch (shape) {
            case Circle circle -> System.gc();
            case Square square -> System.gc();
            case Group group -> System.gc();
        }
    }

    void guardReadsAnEffectivelyFinalLocal(Object value, int limit) {
        switch (value) {
            case Integer number when number > limit -> System.gc();
            case Integer number -> System.gc();
            default -> {
            }
        }
    }
}
