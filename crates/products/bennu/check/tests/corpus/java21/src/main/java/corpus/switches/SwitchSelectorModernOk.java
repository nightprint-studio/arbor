package corpus.switches;

/** Legal twins of {@link SwitchSelectorModernBad}: what level 21 adds to switch selectors and labels. */
public class SwitchSelectorModernOk {
    enum Level {
        LOW, HIGH
    }

    void stringSelectorWithMultipleLabels(String text) {
        switch (text) { case "a", "b" -> System.gc(); default -> { } }
    }

    void boxedLongSelectorWithAGuardedPattern(Long value) {
        switch (value) { case Long big when big > 10 -> System.gc(); default -> { } }
    }

    void objectSelectorWithTypePatterns(Object value) {
        switch (value) { case String text -> System.gc(); case Integer number -> System.gc(); default -> { } }
    }

    void qualifiedEnumLabel(Level level) {
        switch (level) { case Level.LOW -> System.gc(); default -> { } }
    }

    void nullLabel(String text) {
        switch (text) { case null -> System.gc(); case "a" -> System.gc(); default -> { } }
    }

    void arrowEnumStatementNeedNotCoverEveryConstant(Level level) {
        switch (level) { case HIGH -> System.gc(); }
    }
}
