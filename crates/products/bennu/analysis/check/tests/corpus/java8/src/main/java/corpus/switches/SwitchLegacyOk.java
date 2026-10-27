package corpus.switches;

/** Classic `switch` shapes that stay legal on every release. Twin: {@link SwitchConstantsBad}. */
public class SwitchLegacyOk {
    enum Color { RED, GREEN, BLUE }

    static final int ONE = 1;
    static final char LETTER = 'a';
    static final String KEY = "k";

    int fallThroughWithDefaultInTheMiddle(int value) {
        int total = 0;
        switch (value) {
            case 1:
                total += 1;
            default:
                total += 10;
            case 2:
                total += 100;
                break;
            case 3:
                total += 1000;
        }
        return total;
    }

    void narrowAndBoxedSelectors(byte small, short medium, char letter, Character boxedLetter, Integer boxed) {
        switch (small) { case 1: case -128: case 127: break; default: break; }
        switch (medium) { case 300: break; default: break; }
        switch (letter) { case 'a': case 98: break; default: break; }
        switch (boxedLetter) { case 'z': break; default: break; }
        switch (boxed) { case 7: break; default: break; }
    }

    void constantExpressionsAsLabels(int value, String text) {
        final int local = 5;
        switch (value) { case ONE: case local: case ONE + local: case LETTER + 1: case (int) 3.7: case 1 << 4: case Integer.MAX_VALUE: break; default: break; }
        switch (text) { case KEY: case KEY + "2": case "": case "Aa": case "BB": break; default: break; }
    }

    int enumSwitchWithUnqualifiedLabels(Color color) {
        switch (color) {
            case RED:
                return 1;
            case GREEN:
                return 2;
            default:
                return 0;
        }
    }

    int defaultOnly(int value) {
        switch (value) {
            default:
                return 1;
        }
    }

    void emptyAndNestedSwitches(int outer, int inner) {
        switch (outer) { }
        switch (outer) {
            case 1:
                switch (inner) { case 1: break; default: break; }
                break;
            default:
                break;
        }
    }

    void aVariableDeclaredInOneCaseIsInScopeInTheNext(int value) {
        switch (value) {
            case 1:
                int shared = 1;
                System.out.println(shared);
                break;
            case 2:
                shared = 2;
                System.out.println(shared);
                break;
            default:
                break;
        }
    }

    void labeledBreakAndContinueLeaveTheSwitch(int[] values) {
        scan:
        for (int value : values) {
            switch (value) {
                case 0:
                    break scan;
                case 1:
                    continue scan;
                default:
                    System.gc();
            }
        }
    }

    void switchInsideALambda(int value) {
        Runnable task = () -> {
            switch (value) {
                case 1:
                    System.gc();
                    break;
                default:
                    break;
            }
        };
        task.run();
    }
}
