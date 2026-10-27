package corpus.sweep;

/** Increment and decrement keep the operand's type; only unary plus, minus and tilde promote. */
public class Counters {
    private short subroutines;
    private byte small;
    private char letter = 'a';

    static void mark(short id) {
    }

    static void tiny(byte id) {
    }

    static void glyph(char c) {
    }

    void bump() {
        mark(++subroutines);
        mark(subroutines++);
        mark(--subroutines);
        tiny(++small);
        tiny(small--);
        glyph(++letter);
    }
}
