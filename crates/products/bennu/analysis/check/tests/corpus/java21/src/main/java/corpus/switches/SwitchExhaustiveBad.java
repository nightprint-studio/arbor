package corpus.switches;

/** Switches that must be exhaustive and are not. One line per switch. Twin: {@link SwitchExhaustiveOk}. */
public class SwitchExhaustiveBad {
    enum Level {
        LOW, MEDIUM, HIGH
    }

    sealed interface Shape permits Circle, Square {
    }

    record Circle(double radius) implements Shape {
    }

    record Square(double side) implements Shape {
    }

    int enumSwitchExpressionMissingAConstant(Level level) {
        return switch (level) { case LOW -> 1; case MEDIUM -> 2; }; // error: compiler.err.not.exhaustive
    }

    int intSwitchExpressionWithoutADefault(int key) {
        return switch (key) { case 1 -> 1; case 2 -> 2; }; // error: compiler.err.not.exhaustive
    }

    int sealedSwitchExpressionMissingASubtype(Shape shape) {
        return switch (shape) { case Circle circle -> 1; }; // error: compiler.err.not.exhaustive
    }

    void sealedPatternSwitchStatementMissingASubtype(Shape shape) {
        switch (shape) { case Circle circle -> System.gc(); } // error: compiler.err.not.exhaustive.statement
    }

    void objectPatternSwitchStatementWithoutADefault(Object value) {
        switch (value) { case String text -> System.gc(); } // error: compiler.err.not.exhaustive.statement
    }

    void enumStatementWithANullLabelMustBeExhaustive(Level level) {
        switch (level) { case null -> System.gc(); case LOW -> System.gc(); } // error: compiler.err.not.exhaustive.statement
    }
}
