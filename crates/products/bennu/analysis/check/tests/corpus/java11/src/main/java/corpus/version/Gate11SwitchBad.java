package corpus.version;

/**
 * Switch forms newer than Java 11. javac reports a source-level error once per file and feature, so each
 * form appears exactly once, in this order. Twin: {@link Gate11Ok}.
 */
public class Gate11SwitchBad {
    void multipleLabelsInOneCase(int day) {
        switch (day) { case 6, 7: System.gc(); break; default: break; } // error: compiler.err.feature.not.supported.in.source.plural
    }

    void arrowRuleStatement(int day) {
        switch (day) { case 1 -> System.gc(); default -> System.gc(); } // error: compiler.err.feature.not.supported.in.source.plural
    }

    String switchExpression(int day) {
        return switch (day) { case 1 -> "one"; default -> "many"; }; // error: compiler.err.feature.not.supported.in.source.plural
    }
}
