package corpus.switches;

/** Duplicate, non-constant, qualified and ill-typed case labels at level 8. One line per switch. Twin: {@link SwitchLabelOk}. */
public class SwitchLabelBad {
    enum Level {
        LOW, HIGH
    }

    static final String PREFIX = "a";

    void duplicateIntLabel(int value) {
        switch (value) { case 1: break; case 1: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateThroughAConstantExpression(int value) {
        switch (value) { case 2: break; case 1 + 1: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateStringThroughAConstant(String text) {
        switch (text) { case "a": break; case PREFIX: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateEnumLabel(Level level) {
        switch (level) { case LOW: break; case LOW: break; } // error: compiler.err.duplicate.case.label
    }

    void twoDefaults(int value) {
        switch (value) { default: break; default: break; } // error: compiler.err.duplicate.default.label
    }

    void nonConstantIntLabel(int value, int other) {
        switch (value) { case other: break; } // error: compiler.err.const.expr.req
    }

    void nonConstantStringLabel(String text, String other) {
        switch (text) { case other: break; } // error: compiler.err.string.const.req
    }

    void qualifiedEnumLabel(Level level) {
        switch (level) { case Level.LOW: break; } // error: compiler.err.enum.label.must.be.unqualified.enum
    }

    void unknownEnumConstant(Level level) {
        switch (level) { case MEDIUM: break; } // error: compiler.err.enum.label.must.be.unqualified.enum
    }

    void stringLabelForAnIntSelector(int value) {
        switch (value) { case "a": break; } // error: compiler.err.prob.found.req
    }

    void labelOutsideTheByteRange(byte small) {
        switch (small) { case 200: break; } // error: compiler.err.prob.found.req
    }
}
