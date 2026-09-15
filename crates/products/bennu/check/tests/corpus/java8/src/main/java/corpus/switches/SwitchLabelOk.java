package corpus.switches;

/** Legal twins of {@link SwitchLabelBad}. */
public class SwitchLabelOk {
    enum Level {
        LOW, HIGH
    }

    static final String PREFIX = "a";

    void distinctIntLabels(int value) {
        switch (value) { case 1: break; case 2: break; default: break; }
    }

    void groupedLabels(int value) {
        switch (value) { case 1: case 2: break; default: }
    }

    void distinctStringLabels(String text) {
        switch (text) { case PREFIX: break; case "b": break; }
    }

    void distinctEnumLabels(Level level) {
        switch (level) { case LOW: break; case HIGH: break; }
    }

    void enumStatementNeedNotCoverEveryConstant(Level level) {
        switch (level) { case HIGH: System.gc(); }
    }

    void defaultBetweenCases(int value) {
        switch (value) { case 1: break; default: break; case 2: break; }
    }

    void charLabelsForAnIntSelector(int value) {
        switch (value) { case 'a': break; case 'b': break; }
    }

    void emptySwitch(int value) {
        switch (value) { }
    }

    void labelsAtTheEdgesOfTheByteRange(byte small) {
        switch (small) { case -128: break; case 127: break; }
    }
}
