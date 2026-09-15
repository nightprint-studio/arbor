package corpus.switches;

/** Constant labels a level-21 switch does not accept for the selector's type. One line per switch. Twin: {@link SwitchSelectorModernOk}. */
public class SwitchSelectorModernBad {
    void longSelector(long value) {
        switch (value) { case 1 -> System.gc(); default -> { } } // error: compiler.err.selector.type.not.allowed
    }

    void booleanSelector(boolean flag) {
        switch (flag) { case true -> System.gc(); default -> { } } // error: compiler.err.selector.type.not.allowed
    }

    void doubleSelector(double value) {
        switch (value) { case 1.0 -> System.gc(); default -> { } } // error: compiler.err.selector.type.not.allowed
    }

    void stringLabelForAnObjectSelector(Object value) {
        switch (value) { case "a" -> System.gc(); default -> { } } // error: compiler.err.constant.label.not.compatible
    }
}
