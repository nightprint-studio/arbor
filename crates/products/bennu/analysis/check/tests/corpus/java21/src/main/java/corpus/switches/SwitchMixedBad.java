package corpus.switches;

/** A `switch` mixing colon and arrow labels is a syntax error; alone in its file so it cannot muddy another case. */
public class SwitchMixedBad {
    void colonThenArrow(int key) {
        switch (key) { case 1: break; default -> { } } // error: compiler.err.switch.mixing.case.types
    }
}
