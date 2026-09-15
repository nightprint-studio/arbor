package corpus.switches;

/** Legal twins of {@link SwitchPatternBad}. */
public class SwitchPatternOk {
    record Pair(Object left, Object right) {
    }

    void specificPatternBeforeAGeneralOne(Object value) {
        switch (value) { case String text -> System.gc(); case CharSequence text -> System.gc(); default -> { } }
    }

    void guardedPatternBeforeAnUnguardedOne(Object value) {
        switch (value) { case Integer number when number > 0 -> System.gc(); case Integer number -> System.gc(); default -> { } }
    }

    void distinctArrowLabels(int key) {
        switch (key) { case 1, 2 -> System.gc(); case 3 -> System.gc(); default -> { } }
    }

    void patternCompatibleWithTheSelector(CharSequence text) {
        switch (text) { case String exact -> System.gc(); default -> { } }
    }

    void colonFormPatternsWithBreaks(Object value) {
        switch (value) { case String text: System.gc(); break; case Integer number: System.gc(); break; default: break; }
    }

    void nestedRecordPatterns(Object value) {
        switch (value) { case Pair(String left, String right) -> System.gc(); case Pair(Object left, Object right) -> System.gc(); default -> { } }
    }
}
