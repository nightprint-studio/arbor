package corpus.switches;

/** Labels that clash through constant folding, or do not fit the selector. One line per switch. Twin: {@link SwitchLegacyOk}. */
public class SwitchConstantsBad {
    enum Color { RED, GREEN }

    static final int ONE = 1;

    void duplicateThroughANamedConstant(int value) {
        switch (value) { case ONE: break; case 1: break; default: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateCharAndInt(int value) {
        switch (value) { case 'a': break; case 97: break; default: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateThroughFolding(int value) {
        switch (value) { case 2 + 3: break; case 5: break; default: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateStringConcatenation(String text) {
        switch (text) { case "a" + "b": break; case "ab": break; default: break; } // error: compiler.err.duplicate.case.label
    }

    void duplicateEnumConstant(Color color) {
        switch (color) { case RED: break; case RED: break; default: break; } // error: compiler.err.duplicate.case.label
    }

    void labelOutsideTheSelectorRange(byte small) {
        switch (small) { case 200: break; default: break; } // error: compiler.err.prob.found.req
    }

    void labelOfAnotherType(int value) {
        switch (value) { case 1.5: break; default: break; } // error: compiler.err.prob.found.req
    }

    void nonFinalLocalAsLabel(int value) {
        int limit = 3;
        switch (value) { case limit: break; default: break; } // error: compiler.err.const.expr.req
    }
}
