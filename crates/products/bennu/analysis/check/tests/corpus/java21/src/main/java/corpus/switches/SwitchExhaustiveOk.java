package corpus.switches;

/** Legal twins of {@link SwitchExhaustiveBad}: old-style statements need not be exhaustive. */
public class SwitchExhaustiveOk {
    enum Level {
        LOW, MEDIUM, HIGH
    }

    sealed interface Shape permits Circle, Square {
    }

    record Circle(double radius) implements Shape {
    }

    record Square(double side) implements Shape {
    }

    void enumArrowStatementMayMissConstants(Level level) {
        switch (level) { case LOW -> System.gc(); }
    }

    void enumColonStatementMayMissConstants(Level level) {
        switch (level) { case LOW: System.gc(); break; }
    }

    void intStatementMayMissValues(int key) {
        switch (key) { case 1 -> System.gc(); }
    }

    int enumSwitchExpressionCoveringEveryConstant(Level level) {
        return switch (level) { case LOW -> 1; case MEDIUM -> 2; case HIGH -> 3; };
    }

    int intSwitchExpressionWithADefault(int key) {
        return switch (key) { case 1 -> 1; default -> 0; };
    }

    int sealedSwitchExpressionCoveringEverySubtype(Shape shape) {
        return switch (shape) { case Circle circle -> 1; case Square square -> 2; };
    }

    void sealedPatternStatementCoveringEverySubtype(Shape shape) {
        switch (shape) { case Circle circle -> System.gc(); case Square square -> System.gc(); }
    }

    void objectPatternStatementWithADefault(Object value) {
        switch (value) { case String text -> System.gc(); default -> { } }
    }

    int recordPatternsCoveringEverySubtype(Shape shape) {
        return switch (shape) { case Circle(double radius) -> 1; case Square(double side) -> 2; };
    }
}
