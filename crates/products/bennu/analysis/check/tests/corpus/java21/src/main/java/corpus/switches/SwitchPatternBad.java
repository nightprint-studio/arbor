package corpus.switches;

/** Pattern and arrow-label errors: dominance, duplicates, incompatible patterns, fall-through. One line per switch. Twin: {@link SwitchPatternOk}. */
public class SwitchPatternBad {
    void generalPatternBeforeASpecificOne(Object value) {
        switch (value) { case CharSequence text -> System.gc(); case String text -> System.gc(); default -> { } } // error: compiler.err.pattern.dominated
    }

    void unguardedPatternBeforeAGuardedOne(Object value) {
        switch (value) { case Integer number -> System.gc(); case Integer number when number > 0 -> System.gc(); default -> { } } // error: compiler.err.pattern.dominated
    }

    void duplicateArrowLabels(int key) {
        switch (key) { case 1 -> System.gc(); case 1 -> System.gc(); default -> { } } // error: compiler.err.duplicate.case.label
    }

    void duplicateLabelsInOneArm(String text) {
        switch (text) { case "a", "a" -> System.gc(); default -> { } } // error: compiler.err.duplicate.case.label
    }

    void patternIncompatibleWithTheSelector(String text) {
        switch (text) { case Integer number -> System.gc(); default -> { } } // error: compiler.err.prob.found.req
    }

    void fallThroughIntoAPattern(Object value) {
        switch (value) { case String text: System.gc(); case Integer number: System.gc(); break; default: break; } // error: compiler.err.flows.through.to.pattern
    }
}
