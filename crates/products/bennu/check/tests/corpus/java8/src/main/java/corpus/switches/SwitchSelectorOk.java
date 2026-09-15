package corpus.switches;

/** Legal twins of {@link SwitchSelectorBad}: every selector type a level-8 switch accepts. */
public class SwitchSelectorOk {
    enum Level {
        LOW, HIGH
    }

    void intSelector(int value) {
        switch (value) { case 1: break; default: break; }
    }

    void charSelector(char letter) {
        switch (letter) { case 'a': break; case 98: break; }
    }

    void byteAndShortSelectors(byte small, short medium) {
        switch (small) { case 1: break; }
        switch (medium) { case 1: break; }
    }

    void boxedSelectors(Integer number, Character letter) {
        switch (number) { case 1: break; }
        switch (letter) { case 'a': break; }
    }

    void stringSelector(String text) {
        switch (text) { case "a": break; case "b": break; }
    }

    void enumSelector(Level level) {
        switch (level) { case LOW: break; }
    }

    void constantExpressionLabels(int value) {
        final int base = 10;
        switch (value) { case base: break; case base + 1: break; case 'a': break; }
    }
}
