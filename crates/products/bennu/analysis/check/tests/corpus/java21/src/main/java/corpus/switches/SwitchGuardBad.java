package corpus.switches;

/** Guards and label combinations Java 21 rejects. One line per switch. Twin: {@link SwitchModernOk}. */
public class SwitchGuardBad {
    void guardThatIsConstantFalse(Object value) {
        switch (value) { case String text when false -> System.gc(); default -> System.gc(); } // error: compiler.err.guard.has.constant.expression.false
    }

    void unconditionalPatternWithDefault(Object value) {
        switch (value) { case Object any -> System.gc(); default -> System.gc(); } // error: compiler.err.unconditional.pattern.and.default
    }

    void twoUnconditionalPatterns(Object value) {
        switch (value) { case Object first -> System.gc(); case Object second -> System.gc(); } // error: compiler.err.duplicate.unconditional.pattern
    }

    void twoPatternsInOneLabel(Object value) {
        switch (value) { case String text, Integer number -> System.gc(); default -> System.gc(); } // error: compiler.err.flows.through.from.pattern
    }

    void nullBesideAPattern(Object value) {
        switch (value) { case null, String text -> System.gc(); default -> System.gc(); } // error: compiler.err.invalid.case.label.combination
    }
}
