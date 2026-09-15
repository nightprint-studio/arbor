package corpus.switches;

/**
 * Selector types a level-8 switch does not accept. Each switch is on one line: javac reports the
 * selector and the label separately, and both must land on the marked line. Twin: {@link SwitchSelectorOk}.
 */
public class SwitchSelectorBad {
    void longSelector(long value) {
        switch (value) { case 1: break; } // error: compiler.err.selector.type.not.allowed
    }

    void booleanSelector(boolean flag) {
        switch (flag) { case true: break; } // error: compiler.err.selector.type.not.allowed
    }

    void doubleSelector(double value) {
        switch (value) { case 1: break; } // error: compiler.err.selector.type.not.allowed
    }

    void objectSelector(Object value) {
        switch (value) { case "a": break; } // error: compiler.err.feature.not.supported.in.source.plural compiler.err.constant.label.not.compatible compiler.err.not.exhaustive.statement
    }
}
