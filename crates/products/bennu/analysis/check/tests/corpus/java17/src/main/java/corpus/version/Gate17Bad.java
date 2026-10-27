package corpus.version;

/** Pattern forms newer than Java 17, once each (one switch per line). Twin: {@link Gate17Ok}. */
public class Gate17Bad {
    record Point(int x, int y) {
    }

    void patternSwitch(Object value) {
        switch (value) { case String text -> System.gc(); default -> System.gc(); } // error: compiler.err.feature.not.supported.in.source.plural
    }

    void recordPattern(Object value) {
        if (value instanceof Point(int x, int y)) { System.gc(); } // error: compiler.err.feature.not.supported.in.source.plural
    }

    void caseNull(String text) {
        switch (text) { case null -> System.gc(); default -> System.gc(); } // error: compiler.err.feature.not.supported.in.source
    }
}
